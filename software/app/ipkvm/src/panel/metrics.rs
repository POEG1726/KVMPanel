use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    sync::LazyLock,
};

#[derive(Clone)]
struct Metric {
    id: String,
    label: String,
    unit: &'static str,
    min: f64,
    max: f64,
    group: &'static str,
    warn: Option<f64>,
    simulated: Option<f64>,
}

impl Metric {
    fn new(
        id: &str,
        label: &str,
        unit: &'static str,
        range: (f64, f64),
        group: &'static str,
        value: f64,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            unit,
            min: range.0,
            max: range.1,
            group,
            warn: None,
            simulated: Some(value),
        }
    }
}

// Use one catalogue for both paths so IDs, layout ranges and units also work before a PC reports a value.
fn catalogue(t: f64) -> Vec<Metric> {
    let a = ((t / 9.).sin() + 1.) / 2.;
    let b = ((t / 13. + 1.).sin() + 1.) / 2.;
    // Repeatable phase offsets give the PC preview small independent fluctuations.
    let jitter = |i: f64| (t * 1.79 + i * 7.13).sin() * 0.5;
    let cpu_load = 10. + 80. * a + 6. * jitter(1.);
    let cpu_ppt = 40. + 170. * a + 6. * jitter(2.);
    let gpu_power = 40. + 520. * b + 10. * jitter(3.);
    let mut m = vec![
        Metric::new(
            "cpu.clock_eff",
            "Effective clock",
            "GHz",
            (3., 5.8),
            "cpu",
            4.3 + 1.3 * a + 0.05 * jitter(4.),
        ),
        Metric::new(
            "cpu.bclk",
            "BCLK",
            "MHz",
            (99., 101.),
            "cpu",
            100. + 0.04 * jitter(5.),
        ),
        Metric::new(
            "cpu.fclk",
            "FCLK",
            "MHz",
            (1900., 2100.),
            "cpu",
            2000. + 2. * jitter(6.),
        ),
        Metric::new(
            "cpu.vcore",
            "Vcore",
            "V",
            (0.9, 1.4),
            "cpu",
            1. + 0.3 * a + 0.01 * jitter(7.),
        ),
        Metric::new(
            "cpu.tctl",
            "Tctl",
            "°C",
            (30., 95.),
            "cpu",
            42. + 48. * a + 1.5 * jitter(8.),
        ),
        Metric::new("cpu.load", "Load", "%", (0., 100.), "cpu", cpu_load),
        Metric::new("cpu.ppt", "PPT", "W", (0., 230.), "cpu", cpu_ppt),
    ];
    for i in 0..16 {
        let load = cpu_load * if i < 8 { 1.15 } else { 0.7 } + 30. * jitter(i as f64);
        m.push(Metric::new(
            &format!("cpu.core.{i}"),
            &format!("Core {i} (CCD{})", i / 8),
            "%",
            (0., 100.),
            "cpu",
            load.clamp(3., 100.),
        ));
    }
    m.extend([
        Metric::new(
            "gpu.clock",
            "Core",
            "MHz",
            (0., 3000.),
            "gpu",
            900. + 2000. * b + 20. * jitter(9.),
        ),
        Metric::new(
            "gpu.temp",
            "Temp",
            "°C",
            (30., 90.),
            "gpu",
            38. + 34. * b + jitter(10.),
        ),
        Metric::new(
            "gpu.mem_junction",
            "Mem junction",
            "°C",
            (30., 100.),
            "gpu",
            44. + 42. * b + jitter(11.),
        ),
        Metric::new(
            "gpu.load",
            "Load",
            "%",
            (0., 100.),
            "gpu",
            15. + 83. * b + 4. * jitter(12.),
        ),
        Metric::new(
            "gpu.vram_used",
            "VRAM",
            "GB",
            (0., 32.),
            "gpu",
            4. + 20. * b,
        ),
        Metric::new("gpu.power", "Power", "W", (0., 600.), "gpu", gpu_power),
        Metric::new(
            "mem.used",
            "Used",
            "GB",
            (0., 64.),
            "mem",
            24. + 5. * a + 0.2 * jitter(13.),
        ),
    ]);
    for (slot, value) in [
        ("a1", None),
        ("a2", Some(38. + 10. * a + 0.2 * jitter(14.))),
        ("b1", None),
        ("b2", Some(40. + 10. * a + 0.2 * jitter(15.))),
    ] {
        let mut metric = Metric::new(
            &format!("mem.dimm.{slot}"),
            &format!("DIMM {}", slot.to_uppercase()),
            "°C",
            (30., 60.),
            "mem",
            0.,
        );
        metric.simulated = value;
        m.push(metric);
    }
    m.extend([
        Metric::new("mem.mclk", "MCLK", "MHz", (2900., 3100.), "mem", 3000.),
        Metric::new("mem.uclk", "UCLK", "MHz", (2900., 3100.), "mem", 3000.),
        Metric::new(
            "sys.power",
            "System power",
            "W",
            (0., 900.),
            "sys",
            cpu_ppt + gpu_power + 70. + 5. * jitter(16.),
        ),
        Metric::new(
            "ssd.0.temp",
            "SSD 2T",
            "°C",
            (25., 80.),
            "sys",
            38. + 8. * a,
        ),
        Metric::new(
            "ssd.1.temp",
            "990 EVO 4T",
            "°C",
            (25., 80.),
            "sys",
            36. + 6. * b,
        ),
        Metric::new(
            "net.rx",
            "Net ↓",
            "MB/s",
            (0., 120.),
            "sys",
            (40. + 40. * jitter(17.) + 30. * b).max(0.),
        ),
        Metric::new(
            "net.tx",
            "Net ↑",
            "MB/s",
            (0., 40.),
            "sys",
            (5. + 6. * jitter(18.)).max(0.),
        ),
    ]);
    for metric in &mut m {
        metric.warn = match metric.id.as_str() {
            "cpu.tctl" => Some(85.),
            "gpu.mem_junction" => Some(95.),
            id if id.starts_with("mem.dimm.") => Some(60.),
            _ => None,
        };
    }
    m
}

pub type Samples = BTreeMap<String, Option<f64>>;

pub fn snapshot(t: f64, config: &Value, samples: Option<&Samples>) -> Vec<Value> {
    let thresholds = config["thresholds"].as_object();
    let hidden = config["hidden_metrics"].as_array();
    catalogue(t).into_iter().filter_map(|metric| {
        if hidden.is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&metric.id))) {
            return None;
        }
        let value = match samples {
            Some(samples) => samples.get(&metric.id).copied().flatten(),
            None => metric.simulated.map(|v| (v * 1000.).round() / 1000.),
        };
        let mut out = json!({"id": metric.id, "label": metric.label, "unit": metric.unit, "min": metric.min, "max": metric.max, "value": value, "group": metric.group});
        let warn = thresholds.and_then(|map| map.get(&metric.id).or_else(|| {
            map.iter().filter(|(k, _)| k.strip_suffix(".*").is_some_and(|prefix| metric.id.strip_prefix(prefix).is_some_and(|tail| tail.starts_with('.')))).max_by_key(|(k, _)| k.len()).map(|(_, v)| v)
        })).and_then(Value::as_f64).or(metric.warn);
        if let Some(warn) = warn { out["warn"] = json!(warn); }
        Some(out)
    }).collect()
}

pub trait ReportDecoder {
    fn decode(&self, report: &[u8]) -> Option<(String, Option<f64>)>;
}

pub struct TextReportDecoder;
impl ReportDecoder for TextReportDecoder {
    fn decode(&self, report: &[u8]) -> Option<(String, Option<f64>)> {
        if report.len() != 64 || report[0] != 3 {
            return None;
        }
        let payload = &report[1..];
        let end = payload
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(payload.len());
        if payload[end..].iter().any(|b| *b != 0) {
            return None;
        }
        let (id, raw) = std::str::from_utf8(&payload[..end]).ok()?.split_once('=')?;
        static KNOWN: LazyLock<HashSet<String>> =
            LazyLock::new(|| catalogue(0.).into_iter().map(|metric| metric.id).collect());
        if !KNOWN.contains(id) {
            return None;
        }
        let value = if raw == "null" {
            None
        } else {
            let n: f64 = raw.parse().ok()?;
            if !n.is_finite() {
                return None;
            }
            Some(n)
        };
        Some((id.into(), value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_report_requires_exact_id_size_and_a_known_finite_sample() {
        let packet = |line: &[u8]| {
            let mut buf = [0; 64];
            buf[0] = 3;
            buf[1..1 + line.len()].copy_from_slice(line);
            buf
        };
        let mut report = packet(b"cpu.tctl=64.25");
        assert_eq!(
            TextReportDecoder.decode(&report),
            Some(("cpu.tctl".into(), Some(64.25)))
        );
        assert!(TextReportDecoder.decode(&report[..63]).is_none());
        report[0] = 2;
        assert!(TextReportDecoder.decode(&report).is_none());
        for bad in [
            b"cpu.tctl=NaN".as_slice(),
            b"x.y=5",
            b"cpu.tctl=inf",
            b"cpu.tctl=1\0x",
            b"no separator",
        ] {
            assert!(TextReportDecoder.decode(&packet(bad)).is_none());
        }
        assert_eq!(
            TextReportDecoder.decode(&packet(b"mem.dimm.a1=null")),
            Some(("mem.dimm.a1".into(), None))
        );
    }
}
