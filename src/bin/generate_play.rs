use std::{env, path::PathBuf, process::exit, time::SystemTime};

use cheat_ccv3_pp::{Beatmap, GameMode, Mods, OsuPP, OsuStars};

const DEFAULT_PLAYS: usize = 5;

#[derive(Default)]
struct SpecificPlay {
    mods: Option<(String, u32)>,
    combo: Option<u32>,
    misses: Option<u32>,
    n100: Option<u32>,
    n50: Option<u32>,
    accuracy: Option<f64>,
    ar: Option<f32>,
}

impl SpecificPlay {
    fn is_empty(&self) -> bool {
        self.mods.is_none()
            && self.combo.is_none()
            && self.misses.is_none()
            && self.n100.is_none()
            && self.n50.is_none()
            && self.accuracy.is_none()
            && self.ar.is_none()
    }
}

fn print_usage(program: &str) {
    eprintln!("Usage: {program} <beatmap.osu> [plays] [seed] [options]");
    eprintln!("Options:");
    eprintln!("  --mods <mods>       Specify supported mods like HDDT, HR, EZHD, RX, AP, or NoMod");
    eprintln!("  --combo <combo>     Specify play combo");
    eprintln!("  --misses <misses>   Specify play misses");
    eprintln!("  --100s <count>      Specify number of 100s");
    eprintln!("  --50s <count>       Specify number of 50s");
    eprintln!("  --accuracy <acc>    Specify play accuracy in %");
    eprintln!("  --ar <ar>           Override the beatmap AR before mod adjustments");
    eprintln!("  --plays <plays>     Number of plays to generate");
    eprintln!("  --seed <seed>       Seed for random generation of unspecified fields");
    eprintln!("  --all               Show the full play breakdown");
    eprintln!("  -h, --help          Show this help message");
    eprintln!(
        "Example: {program} ./resources/5553026.osu 1 12345 --mods HDDT --misses 150 --accuracy 95.42"
    );
}

struct SimpleRng(u64);

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn gen_range(&mut self, lower: u32, upper: u32) -> u32 {
        if lower >= upper {
            lower
        } else {
            let range = (upper - lower) as u64;
            let value = self.next_u64();
            lower + (value % range) as u32
        }
    }

    fn choose<'a, T>(&mut self, values: &'a [T]) -> &'a T {
        let index = self.gen_range(0, values.len() as u32) as usize;
        &values[index]
    }

    fn choose_optional<'a, T>(&mut self, values: &'a [T]) -> Option<&'a T> {
        if self.gen_range(0, 2) == 0 {
            None
        } else {
            Some(self.choose(values))
        }
    }
}

#[derive(Clone, Copy)]
struct ModOption {
    name: &'static str,
    mod_value: u32,
}

const SPEED_MODS: &[ModOption] = &[
    ModOption {
        name: "NoMod",
        mod_value: 0,
    },
    ModOption {
        name: "DoubleTime",
        mod_value: <u32 as Mods>::DT,
    },
    ModOption {
        name: "HalfTime",
        mod_value: <u32 as Mods>::HT,
    },
];

const DIFFICULTY_MODS: &[ModOption] = &[
    ModOption {
        name: "Easy",
        mod_value: <u32 as Mods>::EZ,
    },
    ModOption {
        name: "HardRock",
        mod_value: <u32 as Mods>::HR,
    },
];

const VISUAL_MODS: &[ModOption] = &[
    ModOption {
        name: "Hidden",
        mod_value: <u32 as Mods>::HD,
    },
    ModOption {
        name: "Flashlight",
        mod_value: <u32 as Mods>::FL,
    },
];

const UTILITY_MODS: &[ModOption] = &[
    ModOption {
        name: "NoFail",
        mod_value: <u32 as Mods>::NF,
    },
    ModOption {
        name: "SpunOut",
        mod_value: <u32 as Mods>::SO,
    },
];

const SPECIAL_MODS: &[ModOption] = &[
    ModOption {
        name: "Relax",
        mod_value: <u32 as Mods>::RX,
    },
    ModOption {
        name: "Autopilot",
        mod_value: <u32 as Mods>::AP,
    },
];

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        exit(1);
    }

    let map_path = PathBuf::from(&args[1]);
    let mut plays = DEFAULT_PLAYS;
    let mut seed = None;
    let mut show_all = false;
    let mut explicit = SpecificPlay::default();
    let mut positional = 0;

    let mut iter = args.iter().skip(2);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage(&args[0]);
                exit(0);
            }
            "--mods" | "-m" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --mods");
                    print_usage(&args[0]);
                    exit(1);
                });
                explicit.mods = Some(parse_mods(value).unwrap_or_else(|| {
                    eprintln!("Invalid mod string: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--combo" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --combo");
                    print_usage(&args[0]);
                    exit(1);
                });
                explicit.combo = Some(value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid combo: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--misses" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --misses");
                    print_usage(&args[0]);
                    exit(1);
                });
                explicit.misses = Some(value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid misses: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--100s" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --100s");
                    print_usage(&args[0]);
                    exit(1);
                });
                explicit.n100 = Some(value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid n100: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--50s" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --50s");
                    print_usage(&args[0]);
                    exit(1);
                });
                explicit.n50 = Some(value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid n50: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--accuracy" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --accuracy");
                    print_usage(&args[0]);
                    exit(1);
                });
                explicit.accuracy = Some(parse_accuracy(value).unwrap_or_else(|| {
                    eprintln!("Invalid accuracy: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--ar" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --ar");
                    print_usage(&args[0]);
                    exit(1);
                });
                let ar = value.parse::<f32>().unwrap_or_else(|_| {
                    eprintln!("Invalid AR: {value}; expected a value from -20 to 20");
                    print_usage(&args[0]);
                    exit(1);
                });
                if !(-20.0..=20.0).contains(&ar) {
                    eprintln!("Invalid AR: {value}; expected a value from -20 to 20");
                    print_usage(&args[0]);
                    exit(1);
                }
                explicit.ar = Some(ar);
            }
            "--plays" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --plays");
                    print_usage(&args[0]);
                    exit(1);
                });
                plays = value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid plays: {value}");
                    print_usage(&args[0]);
                    exit(1);
                });
            }
            "--seed" => {
                let value = iter.next().unwrap_or_else(|| {
                    eprintln!("Missing value for --seed");
                    print_usage(&args[0]);
                    exit(1);
                });
                seed = Some(value.parse().unwrap_or_else(|_| {
                    eprintln!("Invalid seed: {value}");
                    print_usage(&args[0]);
                    exit(1);
                }));
            }
            "--all" => {
                show_all = true;
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unknown option: {arg}");
                print_usage(&args[0]);
                exit(1);
            }
            _ => {
                if positional == 0 {
                    plays = arg.parse().unwrap_or_else(|_| {
                        eprintln!("Invalid plays: {arg}");
                        print_usage(&args[0]);
                        exit(1);
                    });
                    positional += 1;
                } else if positional == 1 {
                    seed = Some(arg.parse().unwrap_or_else(|_| {
                        eprintln!("Invalid seed: {arg}");
                        print_usage(&args[0]);
                        exit(1);
                    }));
                    positional += 1;
                } else {
                    eprintln!("Unexpected argument: {arg}");
                    print_usage(&args[0]);
                    exit(1);
                }
            }
        }
    }

    let seed = seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    });

    let mut rng = SimpleRng::new(seed);
    let map = match Beatmap::from_path(&map_path) {
        Ok(map) => map,
        Err(err) => {
            eprintln!("Failed to parse beatmap '{}': {err}", map_path.display());
            exit(1);
        }
    };

    if map.mode != GameMode::Osu {
        eprintln!("This play generator only supports osu!standard beatmaps");
        exit(1);
    }

    let summary_ar = explicit
        .mods
        .as_ref()
        .map(|(_, mods)| adjusted_ar(&map, *mods, explicit.ar));

    println!("Beatmap: {}", map_path.display());
    println!("Mode: {:?}", map.mode);
    if let Some(ar) = summary_ar {
        println!("AR: {ar:.2}");
    } else {
        println!("Base AR: {:.2}", explicit.ar.unwrap_or(map.ar));
    }
    println!("BPM: {:.2}", map.bpm());
    println!("Total hitobjects: {}", map.hit_objects.len());
    println!("Seed: {seed}");

    if explicit.is_empty() {
        println!("Generating {plays} random plays...\n");
    } else {
        println!("Generating {plays} plays with explicit parameters...\n");
    }

    for index in 1..=plays {
        let (mod_list, mods) = if let Some((ref list, mods)) = explicit.mods {
            (list.clone(), mods)
        } else {
            build_random_mod_combo(&mut rng)
        };
        let ar = adjusted_ar(&map, mods, explicit.ar);
        let diff_attrs = OsuStars::new(&map).mods(mods).calculate();
        let max_combo = diff_attrs.max_combo() as u32;
        let misses = explicit
            .misses
            .unwrap_or_else(|| rng.gen_range(0, (max_combo / 15).max(1) + 1));
        let combo = explicit.combo.unwrap_or_else(|| {
            if misses == 0 {
                max_combo
            } else {
                let lower_combo = (max_combo / 2).max(1);
                rng.gen_range(lower_combo, max_combo + 1)
            }
        });
        let n100 = explicit.n100.unwrap_or_else(|| {
            if misses == 0 {
                0
            } else {
                rng.gen_range(0, (misses + 1).min(10))
            }
        });
        let n50 = explicit.n50.unwrap_or_else(|| {
            if misses == 0 {
                0
            } else {
                rng.gen_range(0, (misses + 1).min(10))
            }
        });
        let accuracy = explicit
            .accuracy
            .unwrap_or_else(|| 90.0 + (rng.gen_range(0, 1001) as f64 / 100.0));

        let mut performance = OsuPP::new(&map)
            .mods(mods)
            .combo(combo as usize)
            .n100(n100 as usize)
            .n50(n50 as usize)
            .n_misses(misses as usize)
            .accuracy(accuracy);
        let score_state = performance.generate_state();
        let perf_attrs = performance.calculate();

        let mut fc_state = score_state.clone();
        fc_state.n300 += fc_state.n_misses;
        fc_state.n_misses = 0;
        fc_state.max_combo = max_combo as usize;
        let fc_attrs = OsuPP::new(&map).mods(mods).state(fc_state).calculate();
        let ss_attrs = OsuPP::new(&map)
            .mods(mods)
            .combo(max_combo as usize)
            .accuracy(100.0)
            .calculate();

        let total_hits =
            score_state.n300 + score_state.n100 + score_state.n50 + score_state.n_misses;
        let achieved_accuracy = if total_hits == 0 {
            0.0
        } else {
            (6 * score_state.n300 + 2 * score_state.n100 + score_state.n50) as f64
                / (6 * total_hits) as f64
        };

        if show_all {
            println!("#{index:02}  {mod_list}");
            print_mod_note(mods);
            println!("  AR: {:.2}", ar);
            println!("  Stars: {:.2}★", perf_attrs.stars());
            println!("  PP: {:.2}", perf_attrs.pp());
            println!("  FC: {:.2} pp", fc_attrs.pp());
            println!("  SS: {:.2} pp", ss_attrs.pp());
            println!(
                "  Aim: {:.2}  Speed: {:.2}  Acc: {:.2}  FL: {:.2}",
                perf_attrs.pp_aim, perf_attrs.pp_speed, perf_attrs.pp_acc, perf_attrs.pp_flashlight
            );
            println!("  Combo: {combo}/{max_combo}");
            println!("  Misses: {}", score_state.n_misses);
            println!(
                "  Accuracy: {accuracy:.2}% -> {:.4}%",
                achieved_accuracy * 100.0
            );
            println!("  300s: {}", score_state.n300);
            println!("  100s: {}", score_state.n100);
            println!("  50s: {}", score_state.n50);
            println!();
        } else {
            println!("Play #{index}");
            println!("  Mods: {mod_list}");
            print_mod_note(mods);
            println!("  AR: {:.2}", ar);
            println!("  Stars: {:.2}★", perf_attrs.stars());
            println!("  Accuracy: {:.4}%", achieved_accuracy * 100.0);
            println!("  Combo: {combo}/{max_combo}");
            println!(
                "  300s: {}  100s: {}  50s: {}",
                score_state.n300, score_state.n100, score_state.n50
            );
            println!("  Misses: {}", score_state.n_misses);
            println!(
                "  PP: {:.2}  |  FC: {:.2}  |  SS: {:.2}\n",
                perf_attrs.pp(),
                fc_attrs.pp(),
                ss_attrs.pp()
            );
        }
    }
}

fn print_mod_note(mods: u32) {
    if mods.rx() {
        println!("  Note: Relax is used for standard PP without cheat-related adjustments.");
    } else {
        println!(
            "  Note: Vanilla PP is adjusted for cheats to represent the real difficulty and skill required."
        );
    }
}

fn adjusted_ar(map: &Beatmap, mods: u32, ar: Option<f32>) -> f64 {
    let mut attributes = map.attributes();
    attributes.mods(mods);

    if let Some(ar) = ar {
        attributes.ar(ar);
    }

    attributes.build().ar
}

fn build_random_mod_combo(rng: &mut SimpleRng) -> (String, u32) {
    let speed = rng.choose(SPEED_MODS);
    let difficulty = rng.choose_optional(DIFFICULTY_MODS);
    let visual = rng.choose_optional(VISUAL_MODS);
    let utility = rng.choose_optional(UTILITY_MODS);
    let special = rng.choose_optional(SPECIAL_MODS);
    let mut mods = speed.mod_value;
    let mut names = vec![speed.name.to_owned()];

    if let Some(difficulty) = difficulty {
        mods |= difficulty.mod_value;
        names.push(difficulty.name.to_owned());
    }

    if let Some(visual) = visual {
        mods |= visual.mod_value;
        names.push(visual.name.to_owned());
    }

    if let Some(utility) = utility {
        mods |= utility.mod_value;
        names.push(utility.name.to_owned());
    }

    if let Some(special) = special {
        if special.mod_value != <u32 as Mods>::AP || mods & <u32 as Mods>::RX == 0 {
            mods |= special.mod_value;
            names.push(special.name.to_owned());
        }
    }

    if names.len() == 1 && names[0] == "NoMod" {
        names[0] = "NoMod".to_owned();
    }

    let mod_list = if names.iter().all(|name| name == "NoMod") {
        "NoMod".to_string()
    } else {
        names
            .into_iter()
            .filter(|name| name != "NoMod")
            .collect::<Vec<String>>()
            .join(" + ")
    };

    (mod_list, mods)
}

fn parse_mods(input: &str) -> Option<(String, u32)> {
    let mut s = input.to_lowercase();
    s.retain(|c| !c.is_whitespace() && c != '+' && c != ',' && c != '/');

    if s.is_empty() {
        return None;
    }

    let mut mods = 0;
    let mut names: Vec<String> = Vec::new();

    if s == "nomod" || s == "nm" {
        return Some(("NoMod".to_string(), mods));
    }

    while !s.is_empty() {
        let next = if let Some(rest) = s.strip_prefix("doubletime") {
            mods |= <u32 as Mods>::DT;
            names.push("DoubleTime".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("halftime") {
            mods |= <u32 as Mods>::HT;
            names.push("HalfTime".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("autopilot") {
            mods |= <u32 as Mods>::AP;
            names.push("Autopilot".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("flashlight") {
            mods |= <u32 as Mods>::FL;
            names.push("Flashlight".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("hardrock") {
            mods |= <u32 as Mods>::HR;
            names.push("HardRock".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("nofail") {
            mods |= <u32 as Mods>::NF;
            names.push("NoFail".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("spunout") {
            mods |= <u32 as Mods>::SO;
            names.push("SpunOut".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("hidden") {
            mods |= <u32 as Mods>::HD;
            names.push("Hidden".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("relax") {
            mods |= <u32 as Mods>::RX;
            names.push("Relax".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("easy") {
            mods |= <u32 as Mods>::EZ;
            names.push("Easy".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("dt") {
            mods |= <u32 as Mods>::DT;
            names.push("DoubleTime".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("ht") {
            mods |= <u32 as Mods>::HT;
            names.push("HalfTime".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("hr") {
            mods |= <u32 as Mods>::HR;
            names.push("HardRock".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("hd") {
            mods |= <u32 as Mods>::HD;
            names.push("Hidden".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("nf") {
            mods |= <u32 as Mods>::NF;
            names.push("NoFail".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("ez") {
            mods |= <u32 as Mods>::EZ;
            names.push("Easy".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("fl") {
            mods |= <u32 as Mods>::FL;
            names.push("Flashlight".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("so") {
            mods |= <u32 as Mods>::SO;
            names.push("SpunOut".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("rx") {
            mods |= <u32 as Mods>::RX;
            names.push("Relax".to_string());
            rest
        } else if let Some(rest) = s.strip_prefix("ap") {
            mods |= <u32 as Mods>::AP;
            names.push("Autopilot".to_string());
            rest
        } else {
            return None;
        };

        s = next.to_string();
    }

    let mod_list = if names.is_empty() {
        "NoMod".to_string()
    } else {
        names.into_iter().collect::<Vec<String>>().join(" + ")
    };

    Some((mod_list, mods))
}

fn parse_accuracy(value: &str) -> Option<f64> {
    let normalized = value.strip_suffix('%').unwrap_or(value);
    let acc = normalized.parse::<f64>().ok()?;
    if (0.0..=100.0).contains(&acc) {
        Some(acc)
    } else {
        None
    }
}
