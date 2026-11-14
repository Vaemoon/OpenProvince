use eframe::egui;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Keyframe {
    pub time: f32,
    pub quat: [f32; 4],
    pub translation: Option<[f32; 3]>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct BoneTrack {
    pub name: String,
    pub bone_id: u32,
    pub frame_type: u32,
    pub keyframes: Vec<Keyframe>,
}

#[allow(dead_code)]
impl BoneTrack {
    pub fn sample(&self, t: f32) -> ([f32; 4], [f32; 3]) {
        if self.keyframes.is_empty() {
            return ([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);
        }
        if self.keyframes.len() == 1 || t <= self.keyframes[0].time {
            let k = &self.keyframes[0];
            return (k.quat, k.translation.unwrap_or([0.0, 0.0, 0.0]));
        }
        let last = &self.keyframes[self.keyframes.len() - 1];
        if t >= last.time {
            return (last.quat, last.translation.unwrap_or([0.0, 0.0, 0.0]));
        }

        let mut idx = 0;
        while idx + 1 < self.keyframes.len() && self.keyframes[idx + 1].time < t {
            idx += 1;
        }

        let k0 = &self.keyframes[idx];
        let k1 = &self.keyframes[idx + 1];
        let dt = (k1.time - k0.time).max(0.0001);
        let factor = ((t - k0.time) / dt).clamp(0.0, 1.0);

        let q0 = k0.quat;
        let mut q1 = k1.quat;
        let dot = q0[0] * q1[0] + q0[1] * q1[1] + q0[2] * q1[2] + q0[3] * q1[3];
        if dot < 0.0 {
            q1 = [-q1[0], -q1[1], -q1[2], -q1[3]];
        }
        let s0 = 1.0 - factor;
        let s1 = factor;
        let mut rx = q0[0] * s0 + q1[0] * s1;
        let mut ry = q0[1] * s0 + q1[1] * s1;
        let mut rz = q0[2] * s0 + q1[2] * s1;
        let mut rw = q0[3] * s0 + q1[3] * s1;
        let len = (rx * rx + ry * ry + rz * rz + rw * rw).sqrt().max(0.00001);
        rx /= len;
        ry /= len;
        rz /= len;
        rw /= len;

        let t0 = k0.translation.unwrap_or([0.0, 0.0, 0.0]);
        let t1 = k1.translation.unwrap_or([0.0, 0.0, 0.0]);
        let tx = t0[0] + (t1[0] - t0[0]) * factor;
        let ty = t0[1] + (t1[1] - t0[1]) * factor;
        let tz = t0[2] + (t1[2] - t0[2]) * factor;

        ([rx, ry, rz, rw], [tx, ty, tz])
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    pub duration: f32,
    pub bones: Vec<BoneTrack>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct IfpPackage {
    pub name: String,
    pub animations: Vec<Animation>,
}

fn read_u32(b: &[u8], offset: usize) -> Option<u32> {
    b.get(offset..offset + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
}

fn read_f32(b: &[u8], offset: usize) -> Option<f32> {
    b.get(offset..offset + 4)
        .and_then(|s| s.try_into().ok())
        .map(f32::from_le_bytes)
}

fn read_str(b: &[u8], offset: usize, max_len: usize) -> String {
    let slice = match b.get(offset..offset + max_len) {
        Some(s) => s,
        None => return String::new(),
    };
    let end = slice.iter().position(|&c| c == 0).unwrap_or(max_len);
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

impl IfpPackage {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 36 {
            return Err("Файл IFP слишком мал".into());
        }

        let magic = bytes.get(0..4).ok_or("Неверный заголовок")?;
        if magic == b"ANP3" {
            Self::parse_anp3(bytes)
        } else if magic == b"ANPK" {
            Self::parse_anpk(bytes)
        } else {
            Err("Неизвестный формат анимации (ожидается ANP3 или ANPK)".into())
        }
    }

    fn parse_anp3(bytes: &[u8]) -> Result<Self, String> {
        let pkg_name = read_str(bytes, 8, 24);
        let num_anims = read_u32(bytes, 32).ok_or("Ошибка чтения числа анимаций")? as usize;
        let mut animations = Vec::with_capacity(num_anims);
        let mut pos = 36usize;

        for _ in 0..num_anims {
            if pos + 36 > bytes.len() {
                break;
            }
            let anim_name = read_str(bytes, pos, 24);
            let num_objects = read_u32(bytes, pos + 24).unwrap_or(0) as usize;
            let frame_data_size = read_u32(bytes, pos + 28).unwrap_or(0) as usize;
            pos += 36;
            let anim_end = pos + frame_data_size;

            let mut bones = Vec::with_capacity(num_objects);
            let mut max_time = 0.0_f32;

            for _ in 0..num_objects {
                if pos + 36 > bytes.len() || pos >= anim_end {
                    break;
                }
                let bone_name = read_str(bytes, pos, 24);
                let frame_type = read_u32(bytes, pos + 24).unwrap_or(0);
                let num_frames = read_u32(bytes, pos + 28).unwrap_or(0) as usize;
                let bone_id = read_u32(bytes, pos + 32).unwrap_or(0);
                pos += 36;

                let mut keyframes = Vec::with_capacity(num_frames);
                for _ in 0..num_frames {
                    if frame_type == 4 {
                        if pos + 32 > bytes.len() {
                            break;
                        }
                        let qx = read_f32(bytes, pos).unwrap_or(0.0);
                        let qy = read_f32(bytes, pos + 4).unwrap_or(0.0);
                        let qz = read_f32(bytes, pos + 8).unwrap_or(0.0);
                        let qw = read_f32(bytes, pos + 12).unwrap_or(1.0);
                        let tx = read_f32(bytes, pos + 16).unwrap_or(0.0);
                        let ty = read_f32(bytes, pos + 20).unwrap_or(0.0);
                        let tz = read_f32(bytes, pos + 24).unwrap_or(0.0);
                        let time = read_f32(bytes, pos + 28).unwrap_or(0.0);
                        pos += 32;
                        max_time = max_time.max(time);
                        keyframes.push(Keyframe {
                            time,
                            quat: [qx, qy, qz, qw],
                            translation: Some([tx, ty, tz]),
                        });
                    } else {
                        if pos + 20 > bytes.len() {
                            break;
                        }
                        let qx = read_f32(bytes, pos).unwrap_or(0.0);
                        let qy = read_f32(bytes, pos + 4).unwrap_or(0.0);
                        let qz = read_f32(bytes, pos + 8).unwrap_or(0.0);
                        let qw = read_f32(bytes, pos + 12).unwrap_or(1.0);
                        let time = read_f32(bytes, pos + 16).unwrap_or(0.0);
                        pos += 20;
                        max_time = max_time.max(time);
                        keyframes.push(Keyframe {
                            time,
                            quat: [qx, qy, qz, qw],
                            translation: None,
                        });
                    }
                }

                bones.push(BoneTrack {
                    name: bone_name,
                    bone_id,
                    frame_type,
                    keyframes,
                });
            }

            animations.push(Animation {
                name: anim_name,
                duration: max_time,
                bones,
            });
            pos = anim_end;
        }

        Ok(Self {
            name: pkg_name,
            animations,
        })
    }

    fn parse_anpk(_bytes: &[u8]) -> Result<Self, String> {
        let pkg_name = "anpk_package".to_string();
        Ok(Self {
            name: pkg_name,
            animations: Vec::new(),
        })
    }
}

pub struct AnimationPlayer {
    pub package: IfpPackage,
    pub selected_anim: usize,
    pub current_time: f32,
    pub playing: bool,
    pub speed: f32,
    pub search: String,
}

impl AnimationPlayer {
    pub fn new(package: IfpPackage) -> Self {
        Self {
            package,
            selected_anim: 0,
            current_time: 0.0,
            playing: true,
            speed: 1.0,
            search: String::new(),
        }
    }

    pub fn advance(&mut self, dt: f32) {
        if !self.playing {
            return;
        }
        let duration = self.current_duration();
        if duration > 0.001 {
            self.current_time += dt * self.speed;
            if self.current_time > duration {
                self.current_time %= duration;
            }
        }
    }

    pub fn current_duration(&self) -> f32 {
        self.package
            .animations
            .get(self.selected_anim)
            .map(|a| a.duration)
            .unwrap_or(1.0)
    }

    pub fn current_anim_name(&self) -> &str {
        self.package
            .animations
            .get(self.selected_anim)
            .map(|a| a.name.as_str())
            .unwrap_or("")
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let duration = self.current_duration();
        let cur_time = self.current_time;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let play_label = if self.playing {
                "Пауза"
            } else {
                "Пуск"
            };
            let play_icon = if self.playing {
                crate::theme::Icon::Pause
            } else {
                crate::theme::Icon::Play
            };
            if crate::theme::tool_button(ui, play_icon, play_label, "Управление анимацией", true)
                .clicked()
            {
                self.playing = !self.playing;
            }
            if crate::theme::tool_button(
                ui,
                crate::theme::Icon::Stop,
                "Сброс",
                "Вернуться к началу анимации",
                true,
            )
            .clicked()
            {
                self.current_time = 0.0;
            }

            ui.add(
                egui::Slider::new(&mut self.current_time, 0.0..=duration.max(0.1))
                    .text(format!("{:.2} / {:.2} с", cur_time, duration))
                    .show_value(false),
            );

            for (spd, label) in [(0.5, "0.5x"), (1.0, "1x"), (2.0, "2x")] {
                if ui
                    .selectable_label((self.speed - spd).abs() < 0.1, label)
                    .clicked()
                {
                    self.speed = spd;
                }
            }

            let anim_count = self.package.animations.len();
            let current_name = self.current_anim_name().to_string();
            egui::ComboBox::from_id_salt("anim_select_combo")
                .selected_text(format!("{} ({})", current_name, anim_count))
                .width(180.0)
                .show_ui(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search).hint_text("Поиск анимации…"),
                    );
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for (idx, anim) in self.package.animations.iter().enumerate() {
                                if !self.search.is_empty()
                                    && !anim
                                        .name
                                        .to_lowercase()
                                        .contains(&self.search.to_lowercase())
                                {
                                    continue;
                                }
                                let is_sel = idx == self.selected_anim;
                                if ui.selectable_label(is_sel, &anim.name).clicked() {
                                    self.selected_anim = idx;
                                    self.current_time = 0.0;
                                }
                            }
                        });
                });
        });
    }
}
