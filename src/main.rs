#![windows_subsystem = "windows"]

use enigo::{Enigo, MouseControllable};
use rand::Rng;
use rdev::{listen, Event, EventType, Key};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use eframe::egui;
use eframe::App;

// Настройки программы
#[derive(Clone)]
struct Settings {
    pause_key: Key,
    resume_key: Key,
    exit_key: Key,
    min_delay_ms: u64,
    max_delay_ms: u64,
    min_pixel_move: i32,
    max_pixel_move: i32,
    axis_bias_percent: i32, // -400..=400 смещение вероятности по оси (%)
    movement_mode: i32, // 0 = X/Y (с bias), 1 = only X, 2 = only Y
}

// Дефолтные значения
fn default_settings() -> Settings {
    Settings {
        pause_key: Key::KeyK,
        resume_key: Key::KeyJ,
        exit_key: Key::KeyP,
        min_delay_ms: 250,
        max_delay_ms: 350,
        min_pixel_move: 1,
        max_pixel_move: 1,
        axis_bias_percent: -400,
        movement_mode: 0, // Дефолт: X/Y с bias
    }
}

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let paused = Arc::new(AtomicBool::new(false));
    let shift_pressed = Arc::new(AtomicBool::new(false));
    let settings = Arc::new(Mutex::new(default_settings()));

    // Поток движения мыши
    {
        let running = running.clone();
        let paused = paused.clone();
        let settings = settings.clone();
        thread::spawn(move || {
            let mut enigo = Enigo::new();
            let mut rng = rand::thread_rng();
			
            // Состояние направления для back-forth режимов
            static mut DIRECTION_X: i32 = 1;
            static mut DIRECTION_Y: i32 = 1;
			
            while running.load(Ordering::SeqCst) {
                if !paused.load(Ordering::SeqCst) {
                    let s = settings.lock().unwrap();

                    // Автоматическая коррекция диапазонов, если min > max
                    let min_delay = s.min_delay_ms.min(s.max_delay_ms);
                    let max_delay = s.min_delay_ms.max(s.max_delay_ms);
                    let min_move = s.min_pixel_move.min(s.max_pixel_move);
                    let max_move = s.min_pixel_move.max(s.max_pixel_move);

                    let dx = rng.gen_range(min_move..=max_move);
                    let dy = rng.gen_range(min_move..=max_move);

                    let (move_x, move_y) = match s.movement_mode {
                        1 => { // only X random
                            let dx = rng.gen_range(min_move..=max_move);
                            (if rng.gen_bool(0.5) { dx } else { -dx }, 0)
                        }
                        2 => { // only Y random
                            let dy = rng.gen_range(min_move..=max_move);
                            (0, if rng.gen_bool(0.5) { dy } else { -dy })
                        }
                        3 => { // only X back-forth
                            let step = s.min_pixel_move;
                            unsafe {
                                let dir = DIRECTION_X;
                                DIRECTION_X = -DIRECTION_X; // Меняем направление
                                (dir * step, 0)
                            }
                        }
                        4 => { // only Y back-forth
                            let step = s.min_pixel_move;
                            unsafe {
                                let dir = DIRECTION_Y;
                                DIRECTION_Y = -DIRECTION_Y; // Меняем направление
                                (0, dir * step)
                            }
                        }
                        _ => { // 0 = X/Y с bias
                            let dx = rng.gen_range(min_move..=max_move);
                            let dy = rng.gen_range(min_move..=max_move);

                            // Вероятность выбора оси с учетом bias
                            let base = 100;
                            let x_prob = base - s.axis_bias_percent.min(0);
                            let y_prob = base + s.axis_bias_percent.max(0);
                            let total_prob = x_prob + y_prob;
                            let roll = rng.gen_range(0..total_prob);

                            if roll < x_prob {
                                // X
                                (if rng.gen_bool(0.5) { dx } else { -dx }, 0)
                            } else {
                                // Y
                                (0, if rng.gen_bool(0.5) { dy } else { -dy })
                            }
                        }
                    };

                    enigo.mouse_move_relative(move_x, move_y);

                    let delay = rng.gen_range(min_delay..=max_delay);
                    drop(s);
                    thread::sleep(Duration::from_millis(delay));
                } else {
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });
    }

    // Поток глобальных клавиш
    {
        let running = running.clone();
        let paused = paused.clone();
        let shift_pressed = shift_pressed.clone();
        let settings = settings.clone();

        thread::spawn(move || {
            if let Err(error) = listen(move |event: Event| {
                let s = settings.lock().unwrap();
                match event.event_type {
                    EventType::KeyPress(Key::ShiftLeft) | EventType::KeyPress(Key::ShiftRight) => {
                        shift_pressed.store(true, Ordering::SeqCst);
                    }
                    EventType::KeyRelease(Key::ShiftLeft) | EventType::KeyRelease(Key::ShiftRight) => {
                        shift_pressed.store(false, Ordering::SeqCst);
                    }
                    EventType::KeyPress(key) => {
                        if key == s.exit_key && shift_pressed.load(Ordering::SeqCst) {
                            running.store(false, Ordering::SeqCst);
                            std::process::exit(0);
                        }

                        // Toggle логика для pause/resume
                        if key == s.pause_key || key == s.resume_key {
                            let current_paused = paused.load(Ordering::SeqCst);
                            paused.store(!current_paused, Ordering::SeqCst);
                            if !current_paused {
                                println!("Cursor movement paused");
                            } else {
                                println!("Cursor movement resumed");
                            }
                        }
                    }
                    _ => {}
                }
            }) {
                eprintln!("Error in global listener: {:?}", error);
            }
        });
    }

    // Запуск GUI
    let app_settings = settings.clone();
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Random Cursor Mover",
        native_options,
        Box::new(move |_cc| Box::new(MyApp { settings: app_settings })),
    );
}

// GUI
struct MyApp {
    settings: Arc<Mutex<Settings>>,
}

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Увеличиваем ширину слайдеров через стиль
        let mut style = (*ctx.style()).clone();
        style.spacing.slider_width = 300.0; // Устанавливаем ширину слайдеров в 300 пикселей
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Cursor Mover Settings");

            let mut s = self.settings.lock().unwrap();

            // Настраиваемые клавиши через ComboBox в отдельных horizontal блоках
            ui.horizontal(|ui| {
                ui.label("Pause key:");
                let pause_options = vec![
                    Key::KeyA, Key::KeyB, Key::KeyC, Key::KeyD, Key::KeyE,
                    Key::KeyF, Key::KeyG, Key::KeyH, Key::KeyI, Key::KeyJ,
                    Key::KeyK, Key::KeyL, Key::KeyM, Key::KeyN, Key::KeyO,
                    Key::KeyP, Key::KeyQ, Key::KeyR, Key::KeyS, Key::KeyT,
                    Key::KeyU, Key::KeyV, Key::KeyW, Key::KeyX, Key::KeyY,
                    Key::KeyZ, Key::F1, Key::F2, Key::F3, Key::F4,
                    Key::F5, Key::F6, Key::F7, Key::F8, Key::F9,
                    Key::F10, Key::F11, Key::F12,
                ];
                egui::ComboBox::from_id_source("pause_combo")
                    .selected_text(format!("{:?}", s.pause_key))
                    .show_ui(ui, |ui| {
                        for option in pause_options {
                            ui.selectable_value(&mut s.pause_key, option, format!("{:?}", option));
                        }
                    });
            });
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Resume key:");
                let resume_options = vec![
                    Key::KeyA, Key::KeyB, Key::KeyC, Key::KeyD, Key::KeyE,
                    Key::KeyF, Key::KeyG, Key::KeyH, Key::KeyI, Key::KeyJ,
                    Key::KeyK, Key::KeyL, Key::KeyM, Key::KeyN, Key::KeyO,
                    Key::KeyP, Key::KeyQ, Key::KeyR, Key::KeyS, Key::KeyT,
                    Key::KeyU, Key::KeyV, Key::KeyW, Key::KeyX, Key::KeyY,
                    Key::KeyZ, Key::F1, Key::F2, Key::F3, Key::F4,
                    Key::F5, Key::F6, Key::F7, Key::F8, Key::F9,
                    Key::F10, Key::F11, Key::F12,
                ];
                egui::ComboBox::from_id_source("resume_combo")
                    .selected_text(format!("{:?}", s.resume_key))
                    .show_ui(ui, |ui| {
                        for option in resume_options {
                            ui.selectable_value(&mut s.resume_key, option, format!("{:?}", option));
                        }
                    });
            });
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Exit key (Shift + key):");
                let exit_options = vec![
                    Key::KeyA, Key::KeyB, Key::KeyC, Key::KeyD, Key::KeyE,
                    Key::KeyF, Key::KeyG, Key::KeyH, Key::KeyI, Key::KeyJ,
                    Key::KeyK, Key::KeyL, Key::KeyM, Key::KeyN, Key::KeyO,
                    Key::KeyP, Key::KeyQ, Key::KeyR, Key::KeyS, Key::KeyT,
                    Key::KeyU, Key::KeyV, Key::KeyW, Key::KeyX, Key::KeyY,
                    Key::KeyZ, Key::F1, Key::F2, Key::F3, Key::F4,
                    Key::F5, Key::F6, Key::F7, Key::F8, Key::F9,
                    Key::F10, Key::F11, Key::F12,
                ];
                egui::ComboBox::from_id_source("exit_combo")
                    .selected_text(format!("{:?}", s.exit_key))
                    .show_ui(ui, |ui| {
                        for option in exit_options {
                            ui.selectable_value(&mut s.exit_key, option, format!("{:?}", option));
                        }
                    });
            });
            ui.add_space(10.0);

            // Слайдеры с увеличенной шириной
            ui.add_space(10.0);
            ui.add(egui::Slider::new(&mut s.min_delay_ms, 50..=5000).text("Min Delay (ms)"));
            ui.add(egui::Slider::new(&mut s.max_delay_ms, 50..=5000).text("Max Delay (ms)"));
            ui.add(egui::Slider::new(&mut s.min_pixel_move, 0..=10).text("Min pixel move (and step for MovementМode 3, 4)"));
            ui.add(egui::Slider::new(&mut s.max_pixel_move, 0..=10).text("Max pixel move"));
            ui.add(egui::Slider::new(&mut s.axis_bias_percent, -400..=400).text("Axis bias (%) (positive: Y more, negative: X more)"));
            ui.add_space(10.0);

            // Новый слайдер для режима движения
            ui.add_space(10.0);
            ui.label("MovementМode (0=random X/Y, 1=random only X, 2=random only Y, 3=Only X back-forth 4=Only Y back-forth)");
            ui.add(egui::Slider::new(&mut s.movement_mode, 0..=4).text("MovementМode"));

            // Кнопка сброса к дефолтным значениям
            ui.add_space(10.0);
            if ui.button("Reset to Default").clicked() {
                *s = default_settings();
                println!("Settings reset to default");
            }
        });
    }
}