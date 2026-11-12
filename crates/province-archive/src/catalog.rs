use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub id: u32,
    pub name: String,
    pub texture: String,
    pub kind: String,
    pub handling: Option<String>,
    pub gxt_key: Option<String>,
}

pub fn parse(text: &str, section: &str) -> Result<Vec<Model>, Error> {
    if !["cars", "peds"].contains(&section) {
        return Err(Error::InvalidArchive(
            "expected cars or peds IDE section".into(),
        ));
    }
    let mut active = false;
    let mut ids = HashSet::new();
    let mut models = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case(section) {
            active = true;
            continue;
        }
        if line.eq_ignore_ascii_case("end") {
            active = false;
            continue;
        }
        if !active {
            continue;
        }
        let fields: Vec<_> = line.split(',').map(str::trim).collect();
        let required = if section == "cars" { 6 } else { 4 };
        if fields.len() < required {
            return Err(Error::InvalidArchive(format!(
                "IDE line {} is incomplete",
                index + 1
            )));
        }
        let id: u32 = fields[0].parse().map_err(|_| {
            Error::InvalidArchive(format!("invalid model ID at line {}", index + 1))
        })?;
        if !ids.insert(id) {
            return Err(Error::InvalidArchive(format!("duplicate model ID {id}")));
        }
        models.push(Model {
            id,
            name: fields[1].into(),
            texture: fields[2].into(),
            kind: fields[3].into(),
            handling: (section == "cars").then(|| fields[4].into()),
            gxt_key: (section == "cars").then(|| fields[5].into()),
        });
    }
    models.sort_by_key(|model| model.id);
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_names_and_ids_survive_without_fabricated_display_names() {
        let text = "# header\ncars\n18785, 100jzx, 100jzx, car, CAR18785, CAR18785\nend\n";
        let models = parse(text, "cars").expect("IDE fixture");
        assert_eq!(models[0].id, 18785);
        assert_eq!(models[0].name, "100jzx");
        assert_eq!(models[0].gxt_key.as_deref(), Some("CAR18785"));
        assert!(parse("cars\n1, a\nend", "cars").is_err());
        assert!(parse("peds\n7,a,a,CIVMALE\n7,b,b,CIVMALE\nend", "peds").is_err());
    }
}
