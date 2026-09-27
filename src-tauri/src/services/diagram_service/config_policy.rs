//! Validate bounded metadata without expanding YAML aliases or passing unknown keys.
use saphyr_parser::{Event, Parser};
use std::{borrow::Cow, collections::HashSet};

pub(super) fn body(source: &str) -> Result<Cow<'_, str>, String> {
    let mut body = source.trim_start();
    if body.lines().next().is_some_and(|line| line.trim() == "---") {
        let start = body.find('\n').ok_or("Mermaid 文首配置缺少结束标记")? + 1;
        let mut end = start;
        let mut found = None;
        for line in body[start..].split_inclusive('\n') {
            if line.trim() == "---" {
                found = Some((end, end + line.len()));
                break;
            }
            end += line.len();
        }
        let (end, next) = found.ok_or("Mermaid 文首配置缺少结束标记")?;
        validate(&body[start..end], false)?;
        body = &body[next..];
    }
    if !body.contains("%%{") {
        return Ok(Cow::Borrowed(body));
    }
    let mut output = String::with_capacity(body.len());
    while let Some(start) = body.find("%%{") {
        output.push_str(&body[..start]);
        let suffix = &body[start + 3..];
        let end = suffix.find("}%%").ok_or("Mermaid directive 缺少结束标记")?;
        validate(&format!("{{{}}}", &suffix[..end]), true)?;
        // Keep a statement boundary after removing configuration for policy inspection.
        output.push('\n');
        body = &suffix[end + 3..];
    }
    output.push_str(body);
    Ok(Cow::Owned(output))
}

fn validate(text: &str, directive: bool) -> Result<(), String> {
    if text.len() > 16 * 1024 {
        return Err("Mermaid 配置超过 16 KiB".into());
    }
    let mut events = Vec::new();
    for (count, event) in Parser::new_from_str(text).enumerate() {
        if count >= 512 {
            return Err("Mermaid 配置超过 512 个解析事件".into());
        }
        let (event, _) = event.map_err(|error| format!("Mermaid 配置 YAML 无效：{error}"))?;
        match event {
            Event::Scalar(_, _, anchor, ref tag) | Event::MappingStart(anchor, ref tag)
                if anchor != 0 || tag.is_some() =>
            {
                return Err("Mermaid 配置禁止 anchor 和 tag".into())
            }
            Event::Alias(_) | Event::SequenceStart(..) => {
                return Err("Mermaid 配置禁止 alias 和数组".into())
            }
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart(_)
            | Event::DocumentEnd => (),
            event => events.push(event),
        }
    }
    let mut index = 0;
    mapping(&events, &mut index, "", directive)?;
    if index != events.len() {
        return Err("Mermaid 配置只允许一个映射".into());
    }
    Ok(())
}

fn mapping(events: &[Event], index: &mut usize, path: &str, directive: bool) -> Result<(), String> {
    if !matches!(events.get(*index), Some(Event::MappingStart(..))) {
        return Err("Mermaid 配置必须是映射".into());
    }
    *index += 1;
    let mut keys = HashSet::new();
    loop {
        if matches!(events.get(*index), Some(Event::MappingEnd)) {
            *index += 1;
            return Ok(());
        }
        let Some(Event::Scalar(key, ..)) = events.get(*index) else {
            return Err("Mermaid 配置键必须是文本".into());
        };
        if !keys.insert(key.to_string()) {
            return Err(format!("Mermaid 配置键重复：{key}"));
        }
        let full = if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        };
        *index += 1;
        let canonical = if directive {
            if full == "init" || full == "initialize" {
                "config".to_string()
            } else if let Some(tail) = full
                .strip_prefix("init.")
                .or_else(|| full.strip_prefix("initialize."))
            {
                format!("config.{tail}")
            } else {
                return Err(format!("Mermaid directive 不支持：{full}"));
            }
        } else {
            full.clone()
        };
        match events.get(*index) {
            Some(Event::MappingStart(..))
                if matches!(
                    canonical.as_str(),
                    "config" | "config.flowchart" | "config.sequence" | "config.themeVariables"
                ) =>
            {
                mapping(events, index, &full, directive)?;
            }
            Some(Event::Scalar(value, ..)) if allowed(&canonical, value) => {
                *index += 1;
            }
            _ => return Err(format!("Mermaid 配置不支持或值超出范围：{canonical}")),
        }
    }
}

fn allowed(path: &str, value: &str) -> bool {
    match path {
        "title" => value.len() <= 512,
        "displayMode" => value == "compact",
        "config.layout" => value == "dagre",
        "config.look" => value == "classic",
        "config.theme" => matches!(value, "default" | "base" | "dark" | "forest" | "neutral"),
        "config.flowchart.curve" => matches!(
            value,
            "basis"
                | "linear"
                | "step"
                | "stepBefore"
                | "stepAfter"
                | "monotoneX"
                | "monotoneY"
                | "natural"
                | "cardinal"
        ),
        "config.flowchart.nodeSpacing" | "config.flowchart.rankSpacing" => {
            value.parse::<u16>().is_ok_and(|v| (1..=200).contains(&v))
        }
        "config.sequence.wrap"
        | "config.sequence.mirrorActors"
        | "config.sequence.showSequenceNumbers" => matches!(value, "true" | "false"),
        "config.themeVariables.primaryColor"
        | "config.themeVariables.primaryTextColor"
        | "config.themeVariables.primaryBorderColor"
        | "config.themeVariables.lineColor"
        | "config.themeVariables.secondaryColor"
        | "config.themeVariables.tertiaryColor" => super::style_policy::color(value),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::body;
    #[test]
    fn accepts_metadata_and_legacy_controlled_config() {
        for source in [
            "---\ntitle: 'URL https://example.test'\nconfig: {layout: dagre, flowchart: {curve: linear}}\n---\nflowchart TD\nA-->B",
            "%%{init: {'theme': 'base', 'themeVariables': {'primaryColor': '#fff'}}}%%\nflowchart TD\nA-->B",
        ] { assert!(body(source).unwrap().trim_start().starts_with("flowchart TD")); }
    }
    #[test]
    fn rejects_unknown_keys_aliases_and_conflicting_values() {
        for config in [
            "config: {securityLevel: loose}",
            "config: {themeCSS: 'body{}'}",
            "config: {layout: unknown}",
            "config: {theme: dark, theme: base}",
            "config: &x {theme: dark}",
            "title: !!str text",
            "config: [dark]",
            "config: {themeVariables: {primaryColor: 'url(https://example.test)'}}",
            "config: {flowchart: {nodeSpacing: 65535}}",
            "title: first\n...\ntitle: second",
        ] {
            assert!(
                body(&format!("---\n{config}\n---\nflowchart TD\nA-->B")).is_err(),
                "{config}"
            );
        }
    }
}
