use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use device_query::{DeviceQuery, DeviceState, Keycode, MousePosition};
use dirs::download_dir;
use eframe::{egui, App, CreationContext};

#[derive(Default)]
struct ActivityTracker {
    task_name: String,
    status: String,
    recording: bool,
    start_time: Option<Instant>,
    activity_data: Arc<Mutex<Vec<ActivityRecord>>>,
    timer_complete: bool,
    permission_checked: bool,
    is_macos: bool,
}

struct ActivityRecord {
    timestamp: u64,
    mouse_x: i32,
    mouse_y: i32,
    keys_pressed: Vec<Keycode>,
}

impl App for ActivityTracker {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Activity Tracker");
            
            // Display macOS permission message if needed
            if !self.permission_checked && self.is_macos {
                self.permission_checked = true;
                // Request initial mouse position to trigger permission dialog on macOS
                let device_state = DeviceState::new();
                let _ = device_state.get_mouse();
                
                self.status = "Note: On macOS, you may need to grant permission for input monitoring in System Preferences → Security & Privacy → Privacy → Input Monitoring".to_string();
            }
            
            ui.horizontal(|ui| {
                ui.label("Task Name: ");
                ui.text_edit_singleline(&mut self.task_name);
            });
            
            ui.add_space(10.0);
            
            ui.horizontal(|ui| {
                if !self.recording {
                    if ui.button("Create Task").clicked() && !self.task_name.is_empty() {
                        self.status = "Preparing to record (5 second countdown)...".to_string();
                        self.start_time = Some(Instant::now());
                        self.recording = true;
                        self.timer_complete = false;
                        self.activity_data = Arc::new(Mutex::new(Vec::new()));
                        
                        // Clone what we need for the background thread
                        let activity_data = Arc::clone(&self.activity_data);
                        
                        // Start background thread for input tracking
                        thread::spawn(move || {
                            // Wait 5 seconds before starting to record
                            thread::sleep(Duration::from_secs(5));
                            
                            let device_state = DeviceState::new();
                            
                            loop {
                                let timestamp = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                                
                                let mouse_pos = device_state.get_mouse().coords;
                                let keys = device_state.get_keys();
                                
                                // Record current state
                                let record = ActivityRecord {
                                    timestamp,
                                    mouse_x: mouse_pos.0,
                                    mouse_y: mouse_pos.1,
                                    keys_pressed: keys,
                                };
                                
                                // Store the record
                                if let Ok(mut data) = activity_data.lock() {
                                    data.push(record);
                                }
                                
                                // Record at 10Hz
                                thread::sleep(Duration::from_millis(100));
                            }
                        });
                    }
                } else {
                    if ui.button("End Task").clicked() {
                        if let Some(start_time) = self.start_time {
                            if start_time.elapsed().as_secs() >= 5 {
                                self.save_activity_data();
                                self.recording = false;
                                self.status = "Recording completed and saved to Downloads folder.".to_string();
                            } else {
                                self.status = "Please wait for timer to complete.".to_string();
                            }
                        }
                    }
                }
            });
            
            ui.add_space(20.0);
            
            // Show timer countdown if recording but timer not complete
            if self.recording && !self.timer_complete {
                if let Some(start_time) = self.start_time {
                    let elapsed = start_time.elapsed().as_secs();
                    if elapsed < 5 {
                        self.status = format!("Recording will start in {} seconds...", 5 - elapsed);
                        // Request repaint to update timer countdown
                        ctx.request_repaint();
                    } else if !self.timer_complete {
                        self.timer_complete = true;
                        self.status = "Recording in progress...".to_string();
                    }
                }
            }
            
            ui.label(&self.status);
            
            // Show macOS specific note if needed
            if self.is_macos {
                ui.add_space(10.0);
                ui.label("⚠️ Note: If inputs aren't recording, check macOS privacy settings.");
            }
        });
    }
}

impl ActivityTracker {
    fn new(_cc: &CreationContext<'_>) -> Self {
        // Detect OS at runtime
        #[allow(unused_mut)]
        let mut is_macos = false;
        
        #[cfg(target_os = "macos")]
        {
            is_macos = true;
        }
        
        Self {
            is_macos,
            permission_checked: false,
            ..Default::default()
        }
    }
    
    fn save_activity_data(&mut self) {
        if let Ok(data) = self.activity_data.lock() {
            if data.is_empty() {
                self.status = "No activity data recorded.".to_string();
                return;
            }
            
            // Create filename with task name and timestamp
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            let sanitized_task_name = self.task_name.replace(' ', "_");
            let filename = format!("{}_{}.csv", sanitized_task_name, timestamp);
            
            // Get downloads directory
            if let Some(download_dir) = download_dir() {
                let file_path: PathBuf = [download_dir.as_path(), Path::new(&filename)].iter().collect();
                
                if let Ok(mut file) = File::create(&file_path) {
                    // Write CSV header
                    writeln!(file, "timestamp,mouse_x,mouse_y,keys_pressed").unwrap();
                    
                    // Write each record
                    for record in data.iter() {
                        let keys_str = record.keys_pressed
                            .iter()
                            .map(|k| format!("{:?}", k))
                            .collect::<Vec<String>>()
                            .join("+");
                        
                        writeln!(
                            file,
                            "{},{},{},\"{}\"",
                            record.timestamp,
                            record.mouse_x,
                            record.mouse_y,
                            keys_str
                        ).unwrap();
                    }
                    
                    let mut status_msg = format!("Activity data saved to {}", file_path.display());
                    if self.is_macos {
                        status_msg.push_str("\nNote: On macOS, you may need to look in ~/Downloads");
                    }
                    self.status = status_msg;
                } else {
                    self.status = "Failed to create output file.".to_string();
                }
            } else {
                self.status = "Could not find Downloads directory.".to_string();
            }
        }
    }
}

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 300.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "Activity Tracker",
        options,
        Box::new(|cc| Ok(Box::new(ActivityTracker::new(cc)))),
    ).unwrap();
}