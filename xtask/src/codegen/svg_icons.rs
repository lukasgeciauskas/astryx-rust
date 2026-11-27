//! The starter icons of lab's SVGIcon and WeightedSVGIcon, from `spec/svg-icons.json`
//! (upstream's `packages/lab/src/SVGIcon/icons.ts` and generated `weightedIcons.ts`), into
//! `crates/astryx-lab/src/svg_icon/generated/icons.rs`: each starter icon's shapes (element
//! type, attributes in order, role) and each weighted icon's drawings per weight range.

use anyhow::{Context, Result};
use serde_json::Value;

fn shapes(list: &Value, what: &str) -> Result<String> {
    let mut out = String::from("&[\n");
    for shape in list.as_array().with_context(|| format!("{what} shapes"))? {
        let ty = shape["type"].as_str().context("shape type")?;
        let role = match shape["role"].as_str() {
            Some(role) => format!("Some({role:?})"),
            None => "None".to_string(),
        };
        out.push_str(&format!("GenShape {{ ty: {ty:?}, role: {role}, attrs: &["));
        for pair in shape["attrs"].as_array().context("shape attrs")? {
            let name = pair[0].as_str().context("attr name")?;
            let value = pair[1].as_str().context("attr value")?;
            out.push_str(&format!("({name:?}, {value:?}), "));
        }
        out.push_str("] },\n");
    }
    out.push(']');
    Ok(out)
}

fn view_box(icon: &Value) -> String {
    match icon["viewBox"].as_str() {
        Some(v) => format!("Some({v:?})"),
        None => "None".to_string(),
    }
}

pub fn render(spec: &Value) -> Result<String> {
    let mut out = String::from(
        "/// One shape of a starter icon (upstream `IconShape`): its element, its attributes in\n/// order, and its role, when set.\n#[derive(Clone, Copy, Debug)]\npub struct GenShape {\n    pub ty: &'static str,\n    pub role: Option<&'static str>,\n    pub attrs: &'static [(&'static str, &'static str)],\n}\n\n/// A starter icon (upstream `SVGIconDef`) and the name upstream exports it under.\n#[derive(Clone, Copy, Debug)]\npub struct GenIconDef {\n    pub export: &'static str,\n    pub name: &'static str,\n    pub view_box: Option<&'static str>,\n    pub primary: &'static [GenShape],\n    pub secondary: Option<&'static [GenShape]>,\n}\n\n/// One drawing of a weighted icon (upstream `WeightedIconLayer`): the weights it covers and\n/// its filled paths, each with its even-odd flag.\n#[derive(Clone, Copy, Debug)]\npub struct GenLayer {\n    pub from: u16,\n    pub to: u16,\n    pub paths: &'static [(&'static str, bool)],\n}\n\n/// A weighted icon (upstream `WeightedIconDef`) and the name upstream exports it under.\n#[derive(Clone, Copy, Debug)]\npub struct GenWeightedDef {\n    pub export: &'static str,\n    pub name: &'static str,\n    pub view_box: Option<&'static str>,\n    pub layers: &'static [GenLayer],\n}\n\n",
    );
    out.push_str(
        "/// Upstream `starterIcons`, in order.\npub static STARTER_ICONS: &[GenIconDef] = &[\n",
    );
    for icon in spec["starter"].as_array().context("starter icons")? {
        let export = icon["export"].as_str().context("export")?;
        let name = icon["name"].as_str().context("name")?;
        let secondary = match icon.get("secondary") {
            Some(list) => format!("Some({})", shapes(list, "secondary")?),
            None => "None".to_string(),
        };
        out.push_str(&format!(
            "    GenIconDef {{ export: {export:?}, name: {name:?}, view_box: {}, primary: {}, secondary: {secondary} }},\n",
            view_box(icon),
            shapes(&icon["primary"], "primary")?
        ));
    }
    out.push_str("];\n\n/// Upstream `weightedStarterIcons`, in order.\npub static WEIGHTED_ICONS: &[GenWeightedDef] = &[\n");
    for icon in spec["weighted"].as_array().context("weighted icons")? {
        let export = icon["export"].as_str().context("export")?;
        let name = icon["name"].as_str().context("name")?;
        out.push_str(&format!(
            "    GenWeightedDef {{ export: {export:?}, name: {name:?}, view_box: {}, layers: &[\n",
            view_box(icon)
        ));
        for layer in icon["layers"].as_array().context("layers")? {
            let from = layer["from"].as_u64().context("from")?;
            let to = layer["to"].as_u64().context("to")?;
            out.push_str(&format!(
                "        GenLayer {{ from: {from}, to: {to}, paths: &["
            ));
            for path in layer["paths"].as_array().context("paths")? {
                let d = path["d"].as_str().context("d")?;
                let even_odd = path["evenOdd"].as_bool().unwrap_or(false);
                out.push_str(&format!("({d:?}, {even_odd}), "));
            }
            out.push_str("] },\n");
        }
        out.push_str("    ] },\n");
    }
    out.push_str("];\n");
    Ok(out)
}
