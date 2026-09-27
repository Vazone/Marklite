//! Convert the fixed renderer's legacy token styles to MathML Core characters.
//! MathML Core only defines mathvariant=normal; other alphabets use Unicode.

pub(super) fn normalize(mut mathml: String) -> String {
    // Plain TeX delimiters do not stretch. Explicit \left/\right already carry
    // stretchy=true and matrix wrappers have explicit prefix/postfix attributes.
    for delimiter in ['(', ')', '[', ']', '{', '}', '|'] {
        mathml = mathml.replace(
            &format!("<mo>{delimiter}</mo>"),
            &format!(r#"<mo stretchy="false">{delimiter}</mo>"#),
        );
    }
    const PREFIX: &str = "<mi mathvariant=\"";
    let mut output = String::with_capacity(mathml.len());
    let mut remaining = mathml.as_str();
    while let Some(start) = remaining.find(PREFIX) {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let Some((variant, tail)) = remaining[PREFIX.len()..].split_once("\">") else {
            break;
        };
        let Some((text, _)) = tail.split_once("</mi>") else {
            break;
        };
        let length = PREFIX.len() + variant.len() + 2 + text.len() + 5;
        let mut chars = text.chars();
        let mapped = chars
            .next()
            .filter(|_| chars.next().is_none())
            .and_then(|ch| alphabet(variant, ch));
        if let Some(ch) = mapped {
            output.push_str("<mi mathvariant=\"normal\">");
            output.push(ch);
            output.push_str("</mi>");
        } else {
            output.push_str(&remaining[..length]);
        }
        remaining = &remaining[length..];
    }
    output.push_str(remaining);
    output
}

fn alphabet(variant: &str, ch: char) -> Option<char> {
    let index = if ch.is_ascii_uppercase() {
        Some(ch as usize - 'A' as usize)
    } else if ch.is_ascii_lowercase() {
        Some(ch as usize - 'a' as usize + 26)
    } else {
        None
    };
    if let Some(index) = index {
        return match variant {
            "bold" => char::from_u32(0x1d400 + index as u32),
            "double-struck" => "𝔸𝔹ℂ𝔻𝔼𝔽𝔾ℍ𝕀𝕁𝕂𝕃𝕄ℕ𝕆ℙℚℝ𝕊𝕋𝕌𝕍𝕎𝕏𝕐ℤ𝕒𝕓𝕔𝕕𝕖𝕗𝕘𝕙𝕚𝕛𝕜𝕝𝕞𝕟𝕠𝕡𝕢𝕣𝕤𝕥𝕦𝕧𝕨𝕩𝕪𝕫"
                .chars()
                .nth(index),
            "script" => "𝒜ℬ𝒞𝒟ℰℱ𝒢ℋℐ𝒥𝒦ℒℳ𝒩𝒪𝒫𝒬ℛ𝒮𝒯𝒰𝒱𝒲𝒳𝒴𝒵𝒶𝒷𝒸𝒹ℯ𝒻ℊ𝒽𝒾𝒿𝓀𝓁𝓂𝓃ℴ𝓅𝓆𝓇𝓈𝓉𝓊𝓋𝓌𝓍𝓎𝓏"
                .chars()
                .nth(index),
            "fraktur" => "𝔄𝔅ℭ𝔇𝔈𝔉𝔊ℌℑ𝔍𝔎𝔏𝔐𝔑𝔒𝔓𝔔ℜ𝔖𝔗𝔘𝔙𝔚𝔛𝔜ℨ𝔞𝔟𝔠𝔡𝔢𝔣𝔤𝔥𝔦𝔧𝔨𝔩𝔪𝔫𝔬𝔭𝔮𝔯𝔰𝔱𝔲𝔳𝔴𝔵𝔶𝔷"
                .chars()
                .nth(index),
            _ => None,
        };
    }
    if variant == "bold" {
        // Unicode Mathematical Bold Greek includes capital theta symbol and
        // final sigma in this order, followed by the mathematical variants.
        return "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡϴΣΤΥΦΧΨΩ∇αβγδεζηθικλμνξοπρςστυφχψω∂ϵϑϰϕϱϖ"
            .chars()
            .position(|base| base == ch)
            .and_then(|i| char::from_u32(0x1d6a8 + i as u32));
    }
    None
}
