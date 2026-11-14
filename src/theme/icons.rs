use eframe::egui::{self, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Icon {
    FolderOpen,
    Plus,
    Replace,
    Trash,
    Save,
    ScanEye,
    Folder,
    Box,
    Image,
    Archive,
    Close,
    ArrowUp,
    File,
    Pencil,
    Undo,
    Redo,
    GitHub,
    Telegram,
    Vk,
    Layers,
    ShieldCheck,
    Install,
    Search,
    Palette,
    Play,
    Stop,
    Pause,
    Gear,
    ChevronDown,
}

pub fn icon(kind: Icon) -> egui::Image<'static> {
    icon_sized(kind, 13.0)
}

pub fn icon_sized(kind: Icon, px: f32) -> egui::Image<'static> {
    let (name, svg) = match kind {
        Icon::FolderOpen => (
            "folder2-open",
            include_str!("../../assets/icons/folder2-open.svg"),
        ),
        Icon::Plus => ("plus-lg", include_str!("../../assets/icons/plus-lg.svg")),
        Icon::Replace => (
            "arrow-left-right",
            include_str!("../../assets/icons/arrow-left-right.svg"),
        ),
        Icon::Trash => (
            "trash3-fill",
            include_str!("../../assets/icons/trash3-fill.svg"),
        ),
        Icon::Save => (
            "floppy-fill",
            include_str!("../../assets/icons/floppy-fill.svg"),
        ),
        Icon::ScanEye => ("eye-fill", include_str!("../../assets/icons/eye-fill.svg")),
        Icon::Folder => (
            "folder-fill",
            include_str!("../../assets/icons/folder-fill.svg"),
        ),
        Icon::Box => ("box-fill", include_str!("../../assets/icons/box-fill.svg")),
        Icon::Image => (
            "image-fill",
            include_str!("../../assets/icons/image-fill.svg"),
        ),
        Icon::Archive => (
            "archive-fill",
            include_str!("../../assets/icons/archive-fill.svg"),
        ),
        Icon::Close => ("x-lg", include_str!("../../assets/icons/x-lg.svg")),
        Icon::ArrowUp => (
            "arrow-up-short",
            include_str!("../../assets/icons/arrow-up-short.svg"),
        ),
        Icon::File => (
            "file-earmark-fill",
            include_str!("../../assets/icons/file-earmark-fill.svg"),
        ),
        Icon::Pencil => (
            "pencil-fill",
            include_str!("../../assets/icons/pencil-fill.svg"),
        ),
        Icon::Undo => (
            "arrow-counterclockwise",
            include_str!("../../assets/icons/arrow-counterclockwise.svg"),
        ),
        Icon::Redo => (
            "arrow-clockwise",
            include_str!("../../assets/icons/arrow-clockwise.svg"),
        ),
        Icon::GitHub => ("github", include_str!("../../assets/icons/github.svg")),
        Icon::Telegram => ("telegram", include_str!("../../assets/icons/telegram.svg")),
        Icon::Vk => (
            "chat-dots-fill",
            include_str!("../../assets/icons/chat-dots-fill.svg"),
        ),
        Icon::Layers => (
            "layers-fill",
            include_str!("../../assets/icons/layers-fill.svg"),
        ),
        Icon::ShieldCheck => (
            "shield-fill-check",
            include_str!("../../assets/icons/shield-fill-check.svg"),
        ),
        Icon::Install => (
            "file-earmark-arrow-down-fill",
            include_str!("../../assets/icons/file-earmark-arrow-down-fill.svg"),
        ),
        Icon::Search => ("search", include_str!("../../assets/icons/search.svg")),
        Icon::Palette => (
            "palette-fill",
            include_str!("../../assets/icons/palette-fill.svg"),
        ),
        Icon::Pause => (
            "pause-fill",
            include_str!("../../assets/icons/pause-fill.svg"),
        ),
        Icon::Stop => (
            "stop-fill",
            include_str!("../../assets/icons/stop-fill.svg"),
        ),
        Icon::Play => (
            "play-fill",
            include_str!("../../assets/icons/play-fill.svg"),
        ),
        Icon::ChevronDown => (
            "chevron-down",
            include_str!("../../assets/icons/chevron-down.svg"),
        ),
        Icon::Gear => (
            "gear-fill",
            include_str!("../../assets/icons/gear-fill.svg"),
        ),
    };
    let bytes = svg.replace("currentColor", "#ffffff").into_bytes();
    egui::Image::from_bytes(format!("bytes://icons/{name}.svg"), bytes)
        .fit_to_exact_size(Vec2::splat(px))
}

pub fn direction_icon(up: bool) -> egui::Image<'static> {
    icon_sized(Icon::ChevronDown, 10.0).rotate(
        if up { std::f32::consts::PI } else { 0.0 },
        Vec2::splat(0.5),
    )
}
