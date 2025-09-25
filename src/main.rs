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
    y_bias_percent: i32, // смещение вероятности по оси Y (%)
}

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let paused = Arc::new(AtomicBool::new(false));
    let shift_pressed = Arc::new(AtomicBool::new(false));
    let settings = Arc::new(Mutex::new(Settings {
        pause_key: Key::KeyK,
        resume_key: Key::KeyJ,   // по умолчанию J
        exit_key: Key::KeyP,
        min_delay_ms: 400,
        max_delay_ms: 500,
        min_pixel_move: 1,
        max_pixel_move: 2,
        y_bias_percent: 200, // 50 по умолчанию Y на 50% чаще, 0 - равновероятно X/Y (50/50), 100 - Y в 2 раза чаще, чем X.
    }));

    // Поток движения мыши
    {
        let running = running.clone();
        let paused = paused.clone();
        let settings = settings.clone();
        thread::spawn(move || {
            let mut enigo = Enigo::new();
            let mut rng = rand::thread_rng();

            while running.load(Ordering::SeqCst) {
                if !paused.load(Ordering::SeqCst) {
                    let s = settings.lock().unwrap();

                    let dx = rng.gen_range(s.min_pixel_move..=s.max_pixel_move);
                    let dy = rng.gen_range(s.min_pixel_move..=s.max_pixel_move);

                    // вероятность выбора оси Y с учетом bias
                    let total = 100 + s.y_bias_percent.max(0);
                    let roll = rng.gen_range(0..total);

                    let (move_x, move_y) = if roll < 100 {
                        // обычная вероятность X
                        (if rng.gen_bool(0.5) { dx } else { -dx }, 0)
                    } else {
                        // Y с повышенной вероятностью
                        (0, if rng.gen_bool(0.5) { dy } else { -dy })
                    };

                    enigo.mouse_move_relative(move_x, move_y);

                    let delay = rng.gen_range(s.min_delay_ms..=s.max_delay_ms);
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

                        if key == s.pause_key {
                            paused.store(true, Ordering::SeqCst);
                            println!("Cursor movement paused");
                        }

                        if key == s.resume_key {
                            paused.store(false, Ordering::SeqCst);
                            println!("Cursor movement resumed");
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

            ui.label(format!("Pause key: {:?}", s.pause_key));
            ui.label(format!("Resume key: {:?}", s.resume_key));
            ui.label(format!("Exit key (Shift + key): {:?}", s.exit_key));

            // Слайдеры с увеличенной шириной
            ui.add_space(10.0); // Добавляем отступ для визуального разделения
            ui.add(egui::Slider::new(&mut s.min_delay_ms, 50..=5000).text("Min Delay (ms)"));
            ui.add(egui::Slider::new(&mut s.max_delay_ms, 50..=5000).text("Max Delay (ms)"));
            ui.add(egui::Slider::new(&mut s.min_pixel_move, 0..=10).text("Min pixel move"));
            ui.add(egui::Slider::new(&mut s.max_pixel_move, 0..=10).text("Max pixel move"));
            ui.add(egui::Slider::new(&mut s.y_bias_percent, 0..=300).text("Y axis bias (%)"));
            ui.add_space(10.0); // Добавляем отступ внизу
        });
    }
}