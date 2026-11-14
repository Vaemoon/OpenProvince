use crate::{
    dff::{Face, Model, is_remap_texture},
    theme,
};
use eframe::{
    egui::{self, Color32, Stroke, Vec2},
    glow::{self, HasContext},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const VERTEX: &str = r#"#version 330 core
layout(location=0) in vec3 position;
layout(location=1) in vec3 normal;
layout(location=2) in vec2 uv;
layout(location=3) in vec4 color;
uniform vec4 camera;
uniform vec2 pan;
uniform mat4 partMatrix;
out vec2 texcoord; out vec3 lightingNormal; out vec4 vertexColor;
void main(){
 float cy=cos(camera.x),sy=sin(camera.x),cp=cos(camera.y),sp=sin(camera.y);
 mat3 r=mat3(cy,-sy*sp,sy*cp, -sy,-cy*sp,cy*cp, 0,cp,sp);
 vec3 p=r*(partMatrix*vec4(position,1.0)).xyz; float depth=3.2+p.z;
 gl_Position=vec4(p.x*camera.z/camera.w+pan.x*depth,p.y*camera.z+pan.y*depth,1.01005025*depth-0.201005,depth);
 texcoord=uv;lightingNormal=r*(mat3(partMatrix)*normal);vertexColor=color;
}"#;
const FRAGMENT: &str = r#"#version 330 core
in vec2 texcoord;in vec3 lightingNormal;in vec4 vertexColor;
uniform sampler2D image;uniform int textured;uniform int vinylOverlay;uniform vec4 material;uniform int alphaPass;uniform int transparency;
out vec4 outputColor;
void main(){
 vec4 t=textured!=0?texture(image,texcoord):vec4(1);
 if(textured!=0 && t.a>0.001)t.rgb/=t.a;
 vec4 c;
 if(vinylOverlay!=0 && textured!=0){
  c=vec4(mix(material.rgb,t.rgb,t.a),material.a);
 }else{
  vec3 prelit=mix(vec3(1.0),vertexColor.rgb,0.45);
  c=vec4(t.rgb*material.rgb*prelit,t.a*material.a*vertexColor.a);
 }
 if(transparency==0 && c.a>0.025)c.a=1.0;
 if(c.a<0.025 || (alphaPass==0 && c.a<0.98) || (alphaPass==1 && c.a>=0.98))discard;
 float diffuse=0.72+0.28*abs(dot(normalize(lightingNormal),normalize(vec3(-0.35,0.6,-0.75))));
 outputColor=vec4(c.rgb*diffuse,c.a);
}"#;

struct Range {
    start: i32,
    count: i32,
    material: usize,
    component: Option<usize>,
    interactable: Option<usize>,
}

const IDENTITY_MAT4: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn part_rotation_matrix(pivot: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 16] {
    if angle.abs() < 1e-4 {
        return IDENTITY_MAT4;
    }
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let [ux, uy, uz] = axis;
    let r00 = c + ux * ux * t;
    let r01 = ux * uy * t - uz * s;
    let r02 = ux * uz * t + uy * s;
    let r10 = uy * ux * t + uz * s;
    let r11 = c + uy * uy * t;
    let r12 = uy * uz * t - ux * s;
    let r20 = uz * ux * t - uy * s;
    let r21 = uz * uy * t + ux * s;
    let r22 = c + uz * uz * t;
    let tx = pivot[0] - (r00 * pivot[0] + r01 * pivot[1] + r02 * pivot[2]);
    let ty = pivot[1] - (r10 * pivot[0] + r11 * pivot[1] + r12 * pivot[2]);
    let tz = pivot[2] - (r20 * pivot[0] + r21 * pivot[1] + r22 * pivot[2]);
    [
        r00, r10, r20, 0.0, r01, r11, r21, 0.0, r02, r12, r22, 0.0, tx, ty, tz, 1.0,
    ]
}
struct Resources {
    gl: Arc<glow::Context>,
    program: glow::Program,
    vao: glow::VertexArray,
    buffer: glow::Buffer,
    ranges: Vec<Range>,
}
impl Resources {
    fn new(gl: Arc<glow::Context>, model: &Model) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, FRAGMENT),
            ] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let error = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    for s in shaders {
                        gl.delete_shader(s);
                    }
                    gl.delete_program(program);
                    return Err(error);
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.link_program(program);
            for s in shaders {
                gl.detach_shader(program, s);
                gl.delete_shader(s);
            }
            if !gl.get_program_link_status(program) {
                let error = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(error);
            }
            let vao = gl.create_vertex_array()?;
            let buffer = gl.create_buffer()?;
            type BucketKey = (Option<usize>, Option<usize>, usize);
            let mut buckets: BTreeMap<BucketKey, Vec<&Face>> = BTreeMap::new();
            for face in &model.faces {
                buckets
                    .entry((face.component, face.interactable, face.material))
                    .or_default()
                    .push(face);
            }
            let mut vertices = Vec::<f32>::with_capacity(model.faces.len() * 36);
            let mut ranges = Vec::with_capacity(buckets.len());
            for ((component, interactable, material), faces) in buckets {
                let start = (vertices.len() / 12) as i32;
                for face in faces {
                    for i in 0..3 {
                        vertices.extend((0..3).map(|axis| {
                            (face.points[i][axis] - model.center[axis]) / model.radius
                        }));
                        vertices.extend(face.normals[i]);
                        vertices.extend(face.uv[i]);
                        vertices.extend(face.colors[i].map(|c| f32::from(c) / 255.0));
                    }
                }
                let count = (vertices.len() / 12) as i32 - start;
                if count > 0 {
                    ranges.push(Range {
                        start,
                        count,
                        material,
                        component,
                        interactable,
                    });
                }
            }
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            let bytes =
                std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), vertices.len() * 4);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
            for (index, size, offset) in [(0, 3, 0), (1, 3, 12), (2, 2, 24), (3, 4, 32)] {
                gl.enable_vertex_attrib_array(index);
                gl.vertex_attrib_pointer_f32(index, size, glow::FLOAT, false, 48, offset);
            }
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            Ok(Self {
                gl,
                program,
                vao,
                buffer,
                ranges,
            })
        }
    }
    fn draw(
        &self,
        painter: &eframe::egui_glow::Painter,
        model: &Model,
        settings: &Settings,
        textures: &[(String, egui::TextureId)],
        vinyl: Option<egui::TextureId>,
        vinyl_on_primary: bool,
    ) {
        let gl = &self.gl;
        let part_mats: Vec<[f32; 16]> = model
            .interactables
            .iter()
            .enumerate()
            .map(|(idx, part)| {
                let frac = settings.part_animated.get(idx).copied().unwrap_or(0.0);
                if frac <= 1e-4 {
                    IDENTITY_MAT4
                } else {
                    let piv = [
                        (part.pivot[0] - model.center[0]) / model.radius,
                        (part.pivot[1] - model.center[1]) / model.radius,
                        (part.pivot[2] - model.center[2]) / model.radius,
                    ];
                    part_rotation_matrix(piv, part.axis, part.max_angle_rad * frac)
                }
            })
            .collect();
        unsafe {
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::CULL_FACE);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "image").as_ref(), 0);
            gl.uniform_1_i32(
                gl.get_uniform_location(self.program, "transparency")
                    .as_ref(),
                i32::from(settings.transparency),
            );
            gl.uniform_4_f32(
                gl.get_uniform_location(self.program, "camera").as_ref(),
                settings.yaw,
                settings.pitch,
                2.4 * settings.zoom,
                settings.aspect,
            );
            gl.uniform_2_f32(
                gl.get_uniform_location(self.program, "pan").as_ref(),
                settings.pan.x,
                settings.pan.y,
            );
            let part_loc = gl.get_uniform_location(self.program, "partMatrix");
            gl.polygon_mode(
                glow::FRONT_AND_BACK,
                if settings.wire {
                    glow::LINE
                } else {
                    glow::FILL
                },
            );
            for pass in 0..2 {
                gl.depth_mask(pass == 0);
                gl.uniform_1_i32(
                    gl.get_uniform_location(self.program, "alphaPass").as_ref(),
                    pass,
                );
                if pass == 0 {
                    gl.disable(glow::BLEND);
                } else {
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                }
                for range in &self.ranges {
                    if range
                        .component
                        .is_some_and(|idx| !settings.components.get(idx).copied().unwrap_or(true))
                    {
                        continue;
                    }
                    let mat4 = range
                        .interactable
                        .and_then(|idx| part_mats.get(idx))
                        .unwrap_or(&IDENTITY_MAT4);
                    gl.uniform_matrix_4_f32_slice(part_loc.as_ref(), false, mat4);
                    let m = &model.materials[range.material];
                    let rgb = [m.color[0], m.color[1], m.color[2]];
                    let primary = matches!(rgb, [60, 255, 0] | [0, 255, 255]);
                    let secondary = matches!(rgb, [255, 0, 175] | [255, 0, 255]);
                    let is_remap = is_remap_texture(&m.texture)
                        || (vinyl_on_primary
                            && primary
                            && !m.texture.contains("bonnet")
                            && !m.texture.contains("disk"));
                    let vinyl_tex = if settings.textured && is_remap {
                        vinyl.and_then(|id| painter.texture(id))
                    } else {
                        None
                    };
                    let texture = if vinyl_tex.is_some() {
                        vinyl_tex
                    } else if settings.textured {
                        textures
                            .iter()
                            .find(|(name, _)| name.eq_ignore_ascii_case(&m.texture))
                            .or_else(|| {
                                textures
                                    .iter()
                                    .find(|(name, _)| crate::txd::name_matches(&m.texture, name))
                            })
                            .and_then(|(_, id)| painter.texture(*id))
                    } else {
                        None
                    };
                    let color = if vinyl_tex.is_some() || primary {
                        settings.primary
                    } else if secondary {
                        settings.secondary
                    } else if texture.is_some()
                        && rgb[0] == rgb[1]
                        && rgb[1] == rgb[2]
                        && (120..=210).contains(&rgb[0])
                    {
                        rgb.map(|c| (f32::from(c) / 255.0 * 1.45).min(1.0))
                    } else {
                        rgb.map(|c| f32::from(c) / 255.0)
                    };
                    gl.uniform_4_f32(
                        gl.get_uniform_location(self.program, "material").as_ref(),
                        color[0],
                        color[1],
                        color[2],
                        f32::from(m.color[3]) / 255.0,
                    );
                    gl.uniform_1_i32(
                        gl.get_uniform_location(self.program, "textured").as_ref(),
                        i32::from(texture.is_some()),
                    );
                    gl.uniform_1_i32(
                        gl.get_uniform_location(self.program, "vinylOverlay")
                            .as_ref(),
                        i32::from(vinyl_tex.is_some()),
                    );
                    gl.bind_texture(glow::TEXTURE_2D, texture);
                    gl.draw_arrays(glow::TRIANGLES, range.start, range.count);
                }
            }
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::FILL);
            gl.depth_mask(true);
            gl.disable(glow::DEPTH_TEST);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }
}
impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_buffer(self.buffer);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_program(self.program);
        }
    }
}
#[derive(Clone)]
struct Settings {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan: Vec2,
    aspect: f32,
    wire: bool,
    textured: bool,
    transparency: bool,
    auto_rotate: bool,
    bg_preset: u8,
    primary: [f32; 3],
    secondary: [f32; 3],
    components: Vec<bool>,
    part_open: Vec<f32>,
    part_animated: Vec<f32>,
}
pub struct View {
    model: Arc<Model>,
    settings: Settings,
    resources: Arc<Mutex<Result<Option<Resources>, String>>>,
}
impl View {
    pub fn new(model: Model) -> Self {
        let components = model
            .components
            .iter()
            .map(|c| c.active_by_default)
            .collect();
        let part_count = model.interactables.len();
        Self {
            model: Arc::new(model),
            settings: Settings {
                yaw: 2.5,
                pitch: -0.3,
                zoom: 1.55,
                pan: Vec2::ZERO,
                aspect: 1.0,
                wire: false,
                textured: true,
                transparency: true,
                auto_rotate: false,
                bg_preset: 0,
                primary: [0.5, 0.53, 0.58],
                secondary: [0.65, 0.65, 0.65],
                components,
                part_open: vec![0.0; part_count],
                part_animated: vec![0.0; part_count],
            },
            resources: Arc::new(Mutex::new(Ok(None))),
        }
    }
    #[cfg(debug_assertions)]
    pub(crate) fn configure_capture(&mut self) {
        if std::env::var_os("OPENPROVINCE_CAPTURE_DOORS").is_some() {
            for (index, part) in self.model.interactables.iter().enumerate() {
                if part.label == "ПЛ дверь" || part.label == "ПП дверь" {
                    self.settings.part_open[index] = 1.0;
                    self.settings.part_animated[index] = 1.0;
                }
            }
        }
        if let Ok(value) = std::env::var("OPENPROVINCE_CAPTURE_YAW")
            && let Ok(yaw) = value.parse::<f32>()
        {
            self.settings.yaw = yaw;
            self.settings.pitch = -0.12;
        }
    }

    fn reset_camera(&mut self) {
        self.settings.yaw = 2.5;
        self.settings.pitch = -0.3;
        self.settings.zoom = 1.55;
        self.settings.pan = Vec2::ZERO;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        textures: &[(String, egui::TextureHandle)],
        paintjob: bool,
        vinyl: Option<&egui::TextureHandle>,
        vinyl_on_primary: bool,
        extra_controls: impl FnOnce(&mut egui::Ui),
    ) -> Option<egui::Response> {
        let active_components = self
            .settings
            .components
            .iter()
            .filter(|&&enabled| enabled)
            .count();
        let total_components = self.model.components.len();
        let open_parts = self
            .settings
            .part_open
            .iter()
            .filter(|&&v| v > 0.02)
            .count();
        let total_parts = self.model.interactables.len();
        let has_paint_materials = self.model.materials.iter().any(|m| {
            matches!(
                [m.color[0], m.color[1], m.color[2]],
                [60, 255, 0] | [255, 0, 175] | [0, 255, 255] | [255, 0, 255]
            )
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
            extra_controls(ui);
            theme::panel_menu(ui, "Отображение", |ui| {
                ui.checkbox(&mut self.settings.textured, "Текстуры");
                ui.checkbox(&mut self.settings.wire, "Каркас");
                ui.checkbox(&mut self.settings.transparency, "Прозрачность");
                ui.checkbox(&mut self.settings.auto_rotate, "Автовращение");
                ui.separator();
                ui.label(
                    egui::RichText::new("Фон сцены")
                        .size(11.5)
                        .color(theme::TEXT_SECONDARY),
                );
                for (preset, label) in [(0, "Тёмный"), (1, "Серый"), (2, "Светлый")]
                {
                    ui.radio_value(&mut self.settings.bg_preset, preset, label);
                }
            });

            theme::panel_menu(ui, "Камера", |ui| {
                for (label, yaw, pitch) in [
                    ("3/4 по умолчанию", 2.5_f32, -0.3_f32),
                    ("Спереди", std::f32::consts::PI, -0.12),
                    ("Сзади", 0.0, -0.12),
                    ("Сбоку", std::f32::consts::FRAC_PI_2, -0.12),
                    ("Сверху", 0.0, -1.48),
                ] {
                    if ui.button(label).clicked() {
                        self.settings.yaw = yaw;
                        self.settings.pitch = pitch;
                        self.settings.pan = Vec2::ZERO;
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Сбросить камеру    R").clicked() {
                    self.reset_camera();
                    ui.close_menu();
                }
            });

            if total_parts > 0 {
                theme::panel_menu(
                    ui,
                    format!("Двери ({open_parts}/{total_parts})"),
                    |ui| {
                        ui.set_min_width(268.0);
                        ui.horizontal(|ui| {
                            if ui.button("Открыть все").clicked() {
                                self.settings.part_open.fill(1.0);
                            }
                            if ui.button("Закрыть все").clicked() {
                                self.settings.part_open.fill(0.0);
                            }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for (val, part) in self
                                    .settings
                                    .part_open
                                    .iter_mut()
                                    .zip(&self.model.interactables)
                                {
                                    ui.horizontal(|ui| {
                                        let mut is_open = *val > 0.02;
                                        if ui.checkbox(&mut is_open, &part.label).changed() {
                                            *val = if is_open { 1.0 } else { 0.0 };
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                theme::percent_slider(ui, val, 124.0);
                                            },
                                        );
                                    });
                                }
                            });
                    },
                );
            }

            if total_components > 0 {
                theme::panel_menu(
                    ui,
                    format!("Детали ({active_components}/{total_components})"),
                    |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Сток").clicked() {
                                for (flag, spec) in self
                                    .settings
                                    .components
                                    .iter_mut()
                                    .zip(&self.model.components)
                                {
                                    *flag = spec.active_by_default;
                                }
                            }
                            if ui.button("Все").clicked() {
                                self.settings.components.fill(true);
                            }
                            if ui.button("Скрыть").clicked() {
                                self.settings.components.fill(false);
                            }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for (flag, spec) in self
                                    .settings
                                    .components
                                    .iter_mut()
                                    .zip(&self.model.components)
                                {
                                    ui.checkbox(flag, &spec.label);
                                }
                            });
                    },
                );
            }

            if self.model.is_vehicle || has_paint_materials || vinyl.is_some() {
                theme::panel_menu(ui, "Кузов", |ui| {
                    ui.set_min_width(200.0);
                    ui.horizontal(|ui| {
                        ui.label("Основной:");
                        ui.color_edit_button_rgb(&mut self.settings.primary);
                    });
                    if has_paint_materials {
                        ui.horizontal(|ui| {
                            ui.label("Вторичный:");
                            ui.color_edit_button_rgb(&mut self.settings.secondary);
                        });
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let presets: &[(&str, [f32; 3])] = &[
                            ("Белый", [0.92, 0.92, 0.94]),
                            ("Серый", [0.5, 0.53, 0.58]),
                            ("Чёрный", [0.12, 0.12, 0.14]),
                            ("Красный", [0.78, 0.16, 0.16]),
                            ("Синий", [0.16, 0.38, 0.78]),
                        ];
                        for &(name, rgb) in presets {
                            if ui.button(name).clicked() {
                                self.settings.primary = rgb;
                            }
                        }
                    });
                });
            }
        });
        ui.add_space(4.0);
        theme::hline(ui);

        if self.model.discarded_triangles > 0 {
            ui.add_space(4.0);
            ui.colored_label(
                theme::STATUS_MODIFIED,
                format!(
                    "В исходнике есть некорректные координаты: пропущено {} треугольников.",
                    self.model.discarded_triangles
                ),
            );
        }
        if let Ok(state) = self.resources.lock()
            && let Err(error) = state.as_ref()
        {
            ui.colored_label(theme::STATUS_REMOVED, error);
            return None;
        }
        ui.add_space(4.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(
                ui.available_width().max(240.0),
                ui.available_height().clamp(240.0, 1200.0),
            ),
            egui::Sense::click_and_drag(),
        );
        let (bg_color, hud_text) = match self.settings.bg_preset {
            1 => (
                Color32::from_rgb(88, 92, 98),
                Color32::from_rgb(232, 234, 238),
            ),
            2 => (
                Color32::from_rgb(214, 217, 222),
                Color32::from_rgb(45, 47, 52),
            ),
            _ => (theme::BG_VIEWPORT, theme::TEXT_SECONDARY),
        };
        ui.painter().rect_filled(rect, 0.0, bg_color);

        let shift = ui.input(|i| i.modifiers.shift);
        let mut interacting = false;
        if response.dragged_by(egui::PointerButton::Primary) && !shift {
            let delta = ui.input(|i| i.pointer.delta());
            self.settings.yaw += delta.x * 0.01;
            self.settings.pitch = (self.settings.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
            interacting = true;
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged_by(egui::PointerButton::Primary) && shift)
        {
            let delta = ui.input(|i| i.pointer.delta());
            self.settings.pan +=
                Vec2::new(delta.x * 2.0 / rect.width(), -delta.y * 2.0 / rect.height());
            interacting = true;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 {
                self.settings.zoom = (self.settings.zoom * (scroll * 0.002).exp()).clamp(0.1, 8.0);
            }
        }
        if response.double_clicked()
            || (!ui.ctx().wants_keyboard_input()
                && ui
                    .ctx()
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::R)))
        {
            self.reset_camera();
        }
        let dt = ui.input(|i| i.stable_dt).min(0.05);
        if self.settings.auto_rotate && !interacting {
            self.settings.yaw += dt * 0.55;
            ui.ctx().request_repaint();
        }
        let mut animating_parts = false;
        for (anim, &target) in self
            .settings
            .part_animated
            .iter_mut()
            .zip(&self.settings.part_open)
        {
            let diff = target - *anim;
            if diff.abs() > 0.002 {
                let step = (dt * 9.0).min(1.0);
                *anim += diff * step;
                animating_parts = true;
            } else {
                *anim = target;
            }
        }
        if animating_parts {
            ui.ctx().request_repaint();
        }
        self.settings.aspect = rect.width() / rect.height().max(1.0);
        let model = self.model.clone();
        let mut settings = self.settings.clone();
        if paintjob && vinyl.is_none() {
            settings.primary = [1.0; 3];
        }
        let resources = self.resources.clone();
        let tex_count = textures.len();
        let vinyl_id = vinyl.map(|h| h.id());
        let textures: Vec<_> = textures
            .iter()
            .map(|(name, handle)| (name.clone(), handle.id()))
            .collect();
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_info, painter| {
                let Ok(mut state) = resources.lock() else {
                    return;
                };
                if matches!(*state, Ok(None)) {
                    *state = Resources::new(painter.gl().clone(), &model).map(Some);
                }
                if let Ok(Some(gpu)) = state.as_ref() {
                    gpu.draw(
                        painter,
                        &model,
                        &settings,
                        &textures,
                        vinyl_id,
                        vinyl_on_primary,
                    );
                }
            })),
        });
        ui.painter().rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0_f32, theme::BORDER_LINE),
            egui::StrokeKind::Inside,
        );

        let stats = format!(
            "Вершины: {} · Треуг.: {} · Текстуры: {}",
            theme::format_thousands(self.model.vertices as u64),
            theme::format_thousands(self.model.faces.len() as u64),
            tex_count
        );
        let font = egui::FontId::proportional(11.5);
        let stats_galley = ui.painter().layout_no_wrap(stats, font, hud_text);
        ui.painter().galley(
            egui::pos2(rect.left() + 8.0, rect.top() + 6.0),
            stats_galley,
            hud_text,
        );
        Some(response)
    }
}
