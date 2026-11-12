use super::{ComponentGroup, ComponentKind, Frame};

pub(super) fn base_frame_name(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((prefix, suffix))
            if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) =>
        {
            prefix
        }
        _ => name,
    }
}

pub(super) fn optional_extra_number(name: &str) -> Option<u8> {
    let base = base_frame_name(name);
    let suffix = base
        .strip_prefix("extra_")
        .or_else(|| base.strip_prefix("extra"))?;
    let n = suffix.parse::<u8>().ok()?;
    (1..=6).contains(&n).then_some(n)
}

pub(super) fn is_permanent_damage_or_lod(name: &str, vehicle: bool) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = base_frame_name(&lower);
    if lower.contains("_dam")
        || (base.len() > 3 && base.ends_with("dam"))
        || (base.len() > 2 && base.ends_with("_d") && base != "misc_d")
    {
        return true;
    }
    vehicle && (base.ends_with("_vlo") || base.ends_with("_lod"))
}

pub(super) fn is_restyle_name(base: &str) -> bool {
    base.contains("_rest")
        || ["_2021", "_2022", "_2023", "_2024", "_2025"]
            .iter()
            .any(|tag| base.contains(tag))
}

fn normalize_cm_label(base: &str) -> (String, String) {
    let trimmed = base
        .strip_prefix("cm_")
        .or_else(|| base.strip_prefix("ug_"))
        .unwrap_or(base)
        .trim_end_matches("_ok")
        .trim_end_matches("_e")
        .trim_end_matches("_a")
        .trim_end_matches("_g");
    let merged = trimmed
        .replace("_lf_", "_")
        .replace("_rf_", "_")
        .replace("_lr_", "_")
        .replace("_rr_", "_");
    let pretty = merged.replace('_', " ");
    (format!("cm:{merged}"), format!("Тюнинг: {pretty}"))
}

fn classify_name(name: &str, has_restyle: bool) -> Option<ComponentGroup> {
    let base = base_frame_name(name);
    if let Some(n) = optional_extra_number(base) {
        return Some(ComponentGroup {
            key: format!("extra{n}"),
            label: format!("Экстра {n}"),
            kind: ComponentKind::Extra,
            active_by_default: false,
        });
    }
    if is_restyle_name(base) {
        let label = if base.contains("_2021") {
            "Рестайлинг 2021"
        } else {
            "Рестайлинг"
        };
        return Some(ComponentGroup {
            key: format!("restyle:{label}"),
            label: label.to_owned(),
            kind: ComponentKind::Restyle,
            active_by_default: false,
        });
    }
    if base.contains("_dorest")
        || (has_restyle
            && ["_2018", "_2019", "_2020"]
                .iter()
                .any(|tag| base.contains(tag)))
    {
        return Some(ComponentGroup {
            key: "dorest".to_owned(),
            label: "Дорестайл / сток".to_owned(),
            kind: ComponentKind::StockVariant,
            active_by_default: true,
        });
    }
    if base.contains("_stock") || base.starts_with("stock_") || base == "bodykit_stock" {
        return Some(ComponentGroup {
            key: "stock".to_owned(),
            label: "Сток-детали".to_owned(),
            kind: ComponentKind::StockVariant,
            active_by_default: true,
        });
    }
    if base.starts_with("cm_")
        && (base.contains("_0_")
            || base.ends_with("_0")
            || base.contains("_pnt_")
            || base == "cm_diffusor_plastic_ok")
    {
        return Some(ComponentGroup {
            key: "stock_cm".to_owned(),
            label: "Базовый обвес".to_owned(),
            kind: ComponentKind::StockVariant,
            active_by_default: true,
        });
    }
    if base.contains("_sport") || base.starts_with("sport_") {
        return Some(ComponentGroup {
            key: "sport".to_owned(),
            label: "Пакет Sport".to_owned(),
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.contains("_carb") {
        return Some(ComponentGroup {
            key: "carbon".to_owned(),
            label: "Карбон".to_owned(),
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.contains("acrap") || base.contains("akrap") {
        return Some(ComponentGroup {
            key: "akrapovic".to_owned(),
            label: "Выхлоп Akrapovic".to_owned(),
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.starts_with("tuning")
        || base.starts_with("tun_")
        || base.contains("_tuning")
        || base.contains("_tun")
        || base.contains("bodykit")
        || base.contains("_kit")
        || base.contains("pandem")
        || base.contains("heritage")
        || base.starts_with("prov_neon_")
        || base == "taxish"
        || base == "windscr_sticker"
        || base.starts_with("window_sticker")
        || base == "momo_rule"
        || base.starts_with("bride_")
        || (!base.ends_with("_0")
            && [
                "fbmp_", "rbmp_", "fbump_", "wg_", "bnt_", "spl_", "exh_", "lgt_", "nto_", "rf_b_",
            ]
            .iter()
            .any(|p| base.starts_with(p)))
    {
        let label = format!("Тюнинг: {}", base.trim_end_matches("_ok").replace('_', " "));
        return Some(ComponentGroup {
            key: format!("tun:{base}"),
            label,
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    if base.starts_with("cm_") || base.starts_with("ug_") {
        let (key, label) = normalize_cm_label(base);
        return Some(ComponentGroup {
            key,
            label,
            kind: ComponentKind::Tuning,
            active_by_default: false,
        });
    }
    None
}

pub(super) fn classify_component(
    mut frame: i32,
    frames: &[Frame],
    has_restyle: bool,
) -> Option<ComponentGroup> {
    for _ in 0..=frames.len() {
        if frame < 0 {
            return None;
        }
        let node = frames.get(frame as usize)?;
        if let Some(group) = classify_name(&node.name, has_restyle) {
            return Some(group);
        }
        frame = node.parent;
    }
    None
}
