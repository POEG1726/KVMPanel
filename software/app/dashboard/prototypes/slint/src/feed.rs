//! Panel socket client: reads NDJSON from kvmd (or the mock), keeps 60 s of history per metric
//! and turns each `telemetry` / `status` message into a plain `Frame` for the UI thread.

use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::time::Duration;

const HISTORY: usize = 60;

pub struct RowData {
    pub kind: i32,
    pub label: String,
    pub value: String,
    pub hot: bool,
    pub line: String,
    pub area: String,
    pub bar: f32,
}

pub struct DimmData {
    pub text: String,
    pub present: bool,
    pub hot: bool,
}

pub struct SeriesData {
    pub name: String,
    pub line: String,
    pub area: String,
    pub color: u32,
}

pub struct Telemetry {
    pub cpu: Vec<RowData>,
    pub gpu: Vec<RowData>,
    pub mem: Vec<RowData>,
    pub sys: Vec<RowData>,
    pub cores: Vec<f32>,
    pub dimms: Vec<DimmData>,
    pub series: Vec<SeriesData>,
    pub dimm_range: String,
    pub clocks: String,
    /// max(cpu.load, gpu.load) in 0..1, drives the background brightness
    pub load: f32,
}

pub enum Update {
    Telemetry(Box<Telemetry>),
    /// (text, state) for ETH, PC, USB chips; state 0 ok, 1 warn, 2 off
    Status([(String, i32); 3]),
    /// PC host program (dis)connected, or the socket itself dropped
    Waiting(bool),
}

struct Metric {
    label: String,
    unit: String,
    value: Option<f64>,
    min: f64,
    max: f64,
    warn: Option<f64>,
    hist: VecDeque<f64>,
}

#[derive(Default)]
struct State {
    metrics: HashMap<String, Metric>,
}

const CPU_ROWS: &[&str] = &["cpu.clock_eff", "cpu.bclk", "cpu.fclk", "cpu.vcore", "cpu.tctl", "cpu.load", "cpu.ppt"];
const GPU_ROWS: &[&str] = &["gpu.clock", "gpu.temp", "gpu.mem_junction", "gpu.load", "gpu.vram_used", "gpu.power"];
const MEM_ROWS: &[&str] = &["mem.used"];
const SYS_ROWS: &[&str] = &["sys.power", "ssd.0.temp", "ssd.1.temp", "net.rx", "net.tx"];
/// Rows followed by a usage bar, as in the mockup.
const BARRED: &[&str] = &["cpu.load", "gpu.load", "gpu.vram_used", "mem.used"];
const DIMMS: [&str; 4] = ["a1", "a2", "b1", "b2"];
const SERIES_COLORS: [u32; 4] = [0xd2a8ff, 0xff9bd2, 0x8bc8ff, 0xffb454];

fn format(id: &str, m: &Metric, v: f64) -> String {
    match (id, m.unit.as_str()) {
        ("cpu.bclk", _) => format!("{v:.2} MHz"),
        ("cpu.tctl", _) => format!("{v:.1} °C"),
        (_, "GHz") => format!("{v:.2} GHz"),
        (_, "V") => format!("{v:.3} V"),
        (_, "GB") => format!("{v:.1} / {} GB", m.max),
        (_, "MB/s") => format!("{v:.1} MB/s"),
        (_, u) => format!("{v:.0} {u}"),
    }
}

fn norm(m: &Metric, v: f64) -> f64 {
    ((v - m.min) / (m.max - m.min)).clamp(0.0, 1.0)
}

/// SVG path strings (line, closed area) in a `w` x `h` viewbox, same mapping as the mockup.
fn paths(m: &Metric, w: f64, h: f64) -> (String, String) {
    if m.hist.len() < 2 {
        return (String::new(), String::new());
    }
    let (top, span) = (h - 2.0, h - 4.0);
    let pts: Vec<(f64, f64)> = m
        .hist
        .iter()
        .enumerate()
        .map(|(i, &v)| (1.0 + i as f64 * (w - 2.0) / (HISTORY - 1) as f64, top - norm(m, v) * span))
        .collect();
    let joined: Vec<String> = pts.iter().map(|(x, y)| format!("{x:.1} {y:.1}")).collect();
    let joined = joined.join(" L");
    let (first_x, last_x, base) = (pts[0].0, pts[pts.len() - 1].0, h - 1.0);
    let line = format!("M{joined}");
    let area = format!("M{first_x:.1} {base} L{joined} L{last_x:.1} {base} Z");
    (line, area)
}

impl State {
    fn apply(&mut self, metrics: &[Value]) {
        for j in metrics {
            let Some(id) = j["id"].as_str() else { continue };
            let value = j["value"].as_f64();
            let m = self.metrics.entry(id.to_string()).or_insert_with(|| Metric {
                label: String::new(),
                unit: String::new(),
                value: None,
                min: 0.0,
                max: 1.0,
                warn: None,
                hist: VecDeque::with_capacity(HISTORY),
            });
            m.label = j["label"].as_str().unwrap_or(id).to_string();
            m.unit = j["unit"].as_str().unwrap_or("").to_string();
            m.min = j["min"].as_f64().unwrap_or(0.0);
            m.max = j["max"].as_f64().unwrap_or(1.0);
            m.warn = j["warn"].as_f64();
            m.value = value;
            if let Some(v) = value {
                if m.hist.len() == HISTORY {
                    m.hist.pop_front();
                }
                m.hist.push_back(v);
            }
        }
    }

    fn val(&self, id: &str) -> Option<f64> {
        self.metrics.get(id).and_then(|m| m.value)
    }

    fn rows(&self, ids: &[&str]) -> Vec<RowData> {
        let mut out = Vec::new();
        for &id in ids {
            let Some(m) = self.metrics.get(id) else { continue };
            let (line, area) = paths(m, 84.0, 18.0);
            out.push(RowData {
                kind: 0,
                label: m.label.clone(),
                value: m.value.map_or("--".into(), |v| format(id, m, v)),
                hot: matches!((m.value, m.warn), (Some(v), Some(w)) if v > w),
                line,
                area,
                bar: 0.0,
            });
            if BARRED.contains(&id) {
                out.push(RowData {
                    kind: 1,
                    label: String::new(),
                    value: String::new(),
                    hot: false,
                    line: String::new(),
                    area: String::new(),
                    bar: m.value.map_or(0.0, |v| norm(m, v) as f32),
                });
            }
        }
        out
    }

    fn telemetry(&self) -> Telemetry {
        let mut dimms = Vec::new();
        let mut series = Vec::new();
        let mut range = None;
        for slot in DIMMS {
            let id = format!("mem.dimm.{slot}");
            let m = self.metrics.get(&id);
            let v = m.and_then(|m| m.value);
            dimms.push(DimmData {
                text: v.map_or("--".into(), |v| format!("{v:.1}")),
                present: v.is_some(),
                hot: matches!((v, m.and_then(|m| m.warn)), (Some(v), Some(w)) if v > w),
            });
            if let Some(m) = m.filter(|m| !m.hist.is_empty()) {
                let (line, area) = paths(m, 200.0, 58.0);
                series.push(SeriesData { name: slot.to_uppercase(), line, area, color: SERIES_COLORS[series.len()] });
                range.get_or_insert((m.min, m.max));
            }
        }
        let (lo, hi) = range.unwrap_or((30.0, 60.0));
        let clock = |id| self.val(id).map_or("--".into(), |v| format!("{v:.0}"));
        Telemetry {
            cpu: self.rows(CPU_ROWS),
            gpu: self.rows(GPU_ROWS),
            mem: self.rows(MEM_ROWS),
            sys: self.rows(SYS_ROWS),
            cores: (0..16).map(|i| self.val(&format!("cpu.core.{i}")).unwrap_or(0.0) as f32).collect(),
            dimms,
            series,
            dimm_range: format!("{lo:.0}–{hi:.0} °C · last 60 s"),
            clocks: format!("{} / {}", clock("mem.mclk"), clock("mem.uclk")),
            load: (self.val("cpu.load").unwrap_or(0.0).max(self.val("gpu.load").unwrap_or(0.0)) / 100.0) as f32,
        }
    }
}

fn status(j: &Value) -> [(String, i32); 3] {
    let eth = &j["eth"];
    let mbps = eth["mbps"].as_u64().unwrap_or(0);
    let speed = if mbps >= 1000 && mbps % 1000 == 0 {
        format!("{}G", mbps / 1000)
    } else if mbps >= 1000 {
        format!("{:.1}G", mbps as f64 / 1000.0)
    } else {
        format!("{mbps}M")
    };
    let eth = match eth["state"].as_str() {
        Some("up") => (format!("{speed} · {}", eth["ip"].as_str().unwrap_or("?")), 0),
        Some("no_ip") => (format!("{speed} · No IP"), 1),
        _ => ("ETH down".into(), 2),
    };
    let pc = match j["pc"]["state"].as_str() {
        Some("ok") => ("PC".into(), 0),
        Some("partial") => ("PC partial".into(), 1),
        _ => ("PC off".into(), 2),
    };
    let usb = match j["usb_a"]["device"].as_str() {
        Some(d) => (d.to_string(), 0),
        None => ("No USB".into(), 2),
    };
    [eth, pc, usb]
}

/// Connects to `addr` forever (1 s backoff), calling `emit` for every UI-relevant message.
pub fn run(addr: String, emit: impl Fn(Update)) {
    let mut state = State::default();
    loop {
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                eprintln!("panel socket connected: {addr}");
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let Ok(j) = serde_json::from_str::<Value>(&line) else { continue };
                    match j["t"].as_str() {
                        Some("telemetry") => {
                            if let Some(ms) = j["metrics"].as_array() {
                                state.apply(ms);
                                emit(Update::Telemetry(Box::new(state.telemetry())));
                                emit(Update::Waiting(false));
                            }
                        }
                        Some("status") => emit(Update::Status(status(&j))),
                        Some("pc") => emit(Update::Waiting(!j["connected"].as_bool().unwrap_or(false))),
                        _ => {}
                    }
                }
                eprintln!("panel socket closed, reconnecting");
                emit(Update::Waiting(true));
            }
            Err(e) => eprintln!("panel socket {addr}: {e}"),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
