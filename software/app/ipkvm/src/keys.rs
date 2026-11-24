//! T113 case keys on the shared KEY_ADC resistor ladder.
use crate::{hardware, panel, App};
use serde_json::json;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);
const LONG: Duration = Duration::from_millis(700);
// 12-bit GPADC with 1800 mV full scale. Allow 1.18–1.45 V when learning idle.
const IDLE_RAW: std::ops::RangeInclusive<u16> = 2686..=3299;
// One effective 8-bit count in the 12-bit IIO value, about 7 mV.
const STABLE_COUNTS: u16 = 16;
const CALIBRATION_SAMPLES: u8 = 3;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Plus,
    Minus,
    Both,
    Func,
    PlusFunc,
    MinusFunc,
    #[default]
    Invalid,
}

// Midpoints of the closest edges of adjacent tolerance intervals, highest first.
const WINDOWS: [(f64, State); 7] = [
    (0.931_156, State::Idle),
    (0.768_189, State::Plus),
    (0.605_552, State::Minus),
    (0.491_621, State::Both),
    (0.378_965, State::Func),
    (0.221_834, State::PlusFunc),
    (0.065_932, State::MinusFunc),
];

#[derive(Debug, PartialEq, Eq)]
enum Press {
    Short,
    Long,
}

#[derive(Default)]
struct Keys {
    baseline: Option<f64>,
    steady: (u16, u8),
    candidate: Option<(State, Instant)>,
    confirmed: State,
    // Cleared once an action fires, a read fails, or a chord is confirmed.
    func_since: Option<Instant>,
}

impl Keys {
    fn reset_input(&mut self) {
        self.steady.1 = 0;
        self.candidate = None;
        self.confirmed = State::Invalid;
        self.func_since = None;
    }

    fn sample(&mut self, raw: u16, now: Instant) -> Option<Press> {
        let (first, count) = &mut self.steady;
        if *count == 0 || raw.abs_diff(*first) > STABLE_COUNTS {
            self.steady = (raw, 1);
        } else {
            *count = count.saturating_add(1);
        }
        let steady = self.steady.1 >= CALIBRATION_SAMPLES;
        let value = f64::from(raw);
        let raise = self.baseline.is_none_or(|base| value >= base * 1.06);
        if raise && IDLE_RAW.contains(&raw) {
            // Wait for a stable higher level before treating it as idle. A
            // correction must not look like an ordinary FUNC release.
            self.candidate = None;
            if steady {
                self.baseline = Some(value);
                self.confirmed = State::Idle;
                self.func_since = None;
            }
            return None;
        }

        let baseline = self.baseline?;
        let ratio = value / baseline;
        let state = if raw > *IDLE_RAW.end() {
            State::Invalid
        } else {
            WINDOWS
                .iter()
                .find(|&&(low, _)| ratio >= low)
                .map_or(State::Invalid, |&(_, state)| state)
        };
        let Some((same, since)) = self.candidate.filter(|&(same, _)| same == state) else {
            self.candidate = Some((state, now));
            return None;
        };
        if steady && state == State::Idle && (0.97..=1.03).contains(&ratio) {
            self.baseline = Some(baseline + (value - baseline) / 32.);
        }
        if same != self.confirmed {
            let previous = std::mem::replace(&mut self.confirmed, same);
            if previous == State::Idle && same == State::Func {
                self.func_since = Some(since);
            } else if let Some(down) = self.func_since.take() {
                if same == State::Idle {
                    // The threshold may fall between the last held sample and
                    // the first release sample.
                    return Some(if since.duration_since(down) >= LONG {
                        Press::Long
                    } else {
                        Press::Short
                    });
                }
            }
        }
        if same == State::Func
            && self
                .func_since
                .is_some_and(|down| now.duration_since(down) >= LONG)
        {
            self.func_since = None;
            return Some(Press::Long);
        }
        None
    }
}

fn read_raw(path: &Path) -> io::Result<u16> {
    fs::read_to_string(path)?
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|raw| *raw < 4096)
        .ok_or(io::ErrorKind::InvalidData.into())
}

async fn run_action(app: &App, press: Press) -> Result<(), crate::ApiError> {
    let field = match press {
        Press::Short => "short",
        Press::Long => "long",
    };
    let config = app.config();
    match config["function_key"][field].as_str() {
        Some("stop_mcp") => crate::mcp::stop_from_function_key(app).await,
        Some("pause_kvm") if config["kvm"]["enabled"] == true => {
            let app = app.clone();
            tokio::task::spawn_blocking(move || {
                app.patch_config(&json!({"kvm":{"enabled":false}}), None)
            })
            .await
            .map_err(io::Error::other)??;
            Ok(())
        }
        Some(action @ ("toggle_settings" | "toggle_screen" | "cycle_brightness")) => {
            panel::function_key(app, action).await
        }
        _ => Ok(()),
    }
}

pub async fn run(app: App) {
    let path = match env::var_os("KVMD_KEY_ADC") {
        Some(path) => Some(PathBuf::from(path)),
        None if !app.sim => hardware::key_adc_path(),
        None => return,
    };
    let Some(path) = path.filter(|p| p.is_file()) else {
        eprintln!("kvmd key ADC: GPADC0 raw node unavailable, case keys skipped");
        return;
    };
    let trace = env::var("KVMD_KEY_ADC_TRACE").as_deref() == Ok("1");
    eprintln!("kvmd key ADC: polling {}", path.display());
    let path = std::sync::Arc::new(path);
    let mut tick = tokio::time::interval(POLL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut keys = Keys::default();
    let mut read_failed = false;
    // Serialize actions without blocking ADC sampling on config writes or MCP cleanup.
    let (actions, mut presses) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(press) = presses.recv().await {
            if let Err(error) = run_action(&app, press).await {
                eprintln!("kvmd key ADC: FUNC action failed: {}", error.code);
            }
        }
    });
    loop {
        tick.tick().await;
        let input = path.clone();
        match tokio::task::spawn_blocking(move || read_raw(&input)).await {
            Ok(Ok(raw)) => {
                let previous_state = keys.confirmed;
                let previous_baseline = keys.baseline;
                let press = keys.sample(raw, Instant::now());
                if trace
                    || read_failed
                    || keys.confirmed != previous_state
                    || keys
                        .baseline
                        .zip(previous_baseline)
                        .is_some_and(|(new, old)| new >= old * 1.06)
                {
                    eprintln!(
                        "kvmd key ADC: raw={raw} baseline={:.1} state={:?}",
                        keys.baseline.unwrap_or(0.0),
                        keys.confirmed
                    );
                }
                read_failed = false;
                if let Some(press) = press {
                    eprintln!("kvmd key ADC: FUNC {press:?}");
                    if actions.send(press).is_err() {
                        return;
                    }
                }
            }
            error => {
                if !read_failed {
                    eprintln!("kvmd key ADC: cannot read {}: {error:?}", path.display());
                }
                read_failed = true;
                keys.reset_input();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: [(State, [f64; 2]); 8] = [
        (State::Idle, [1287.568, 1326.179]),
        (State::Plus, [1106.396, 1141.244]),
        (State::Minus, [866.980, 896.023]),
        (State::Both, [688.533, 712.628]),
        (State::Func, [574.114, 594.757]),
        (State::PlusFunc, [399.517, 414.471]),
        (State::MinusFunc, [172.051, 178.821]),
        (State::Invalid, [0., 0.]),
    ];

    fn raw(millivolts: f64) -> u16 {
        (millivolts * 4096. / 1800.).round() as u16
    }

    struct Test {
        keys: Keys,
        time: Instant,
    }

    impl Test {
        fn new() -> Self {
            Self {
                keys: Keys::default(),
                time: Instant::now(),
            }
        }

        fn step(&mut self, raw: u16) -> Option<Press> {
            self.time += POLL;
            self.keys.sample(raw, self.time)
        }

        fn settle(&mut self, raw: u16) {
            for _ in 0..3 {
                assert_eq!(self.step(raw), None);
            }
        }
    }

    #[test]
    fn all_states_with_gain_offset_and_resistor_tolerance() {
        for gain in [0.95, 1.05] {
            for offset in [-10., 10.] {
                let counts = |mv| raw(mv * gain + offset);
                for idle in LEVELS[0].1 {
                    for (state, range) in LEVELS {
                        for mv in range {
                            let mut test = Test::new();
                            let idle_raw = counts(idle);
                            test.settle(idle_raw);
                            assert_eq!(test.keys.baseline, Some(f64::from(idle_raw)));
                            let sample = if state == State::Idle {
                                idle_raw
                            } else {
                                counts(mv)
                            };
                            test.settle(sample);
                            assert_eq!(
                                test.keys.confirmed, state,
                                "{gain}, {offset}, {idle}, {mv}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn learns_stable_idle_and_corrects_held_plus_at_boot() {
        let mut test = Test::new();
        let plus = raw(1123. * 1.05 + 10.);
        let idle = raw(1307. * 1.05 + 10.);
        test.settle(0);
        test.settle(raw(584.));
        assert!(test.keys.baseline.is_none());
        test.step(plus);
        test.step(plus + 20);
        test.step(plus);
        test.step(plus);
        assert!(test.keys.baseline.is_none());
        test.step(plus);
        assert_eq!(test.keys.baseline, Some(f64::from(plus)));
        test.step(idle);
        test.step(idle);
        assert_eq!(test.keys.baseline, Some(f64::from(plus)));
        test.step(idle);
        assert_eq!(test.keys.baseline, Some(f64::from(idle)));
        test.settle(idle + 30);
        let updated = test.keys.baseline.unwrap();
        assert!(updated > f64::from(idle) && updated < f64::from(idle + 5));
    }

    #[test]
    fn confirms_two_samples_then_runs_short_only_after_release() {
        let mut test = Test::new();
        let idle = raw(1307.);
        let func = raw(584.);
        test.settle(idle);
        test.step(func);
        test.settle(idle);
        assert_eq!(test.keys.confirmed, State::Idle);
        test.step(func);
        assert_eq!(test.keys.confirmed, State::Idle);
        test.step(func);
        assert_eq!(test.keys.confirmed, State::Func);
        assert_eq!(test.step(idle), None);
        assert_eq!(test.step(idle), Some(Press::Short));
        assert_eq!(test.step(idle), None);
    }

    #[test]
    fn long_fires_once_at_threshold_and_release_does_not_repeat() {
        let mut test = Test::new();
        let idle = raw(1307.);
        let func = raw(584.);
        test.settle(idle);
        for _ in 0..LONG.as_millis() / POLL.as_millis() {
            assert_eq!(test.step(func), None);
        }
        assert_eq!(test.step(func), Some(Press::Long));
        test.settle(func);
        test.settle(idle);
    }

    #[test]
    fn chord_invalid_and_read_failure_cancel_pending_func() {
        let idle = raw(1307.);
        let func = raw(584.);
        for cancel in [Some(raw(407.)), Some(0), None] {
            let mut test = Test::new();
            test.settle(idle);
            test.settle(func);
            if let Some(value) = cancel {
                test.settle(value);
            } else {
                test.keys.reset_input();
            }
            test.settle(func);
            test.settle(idle);
        }
    }
}
