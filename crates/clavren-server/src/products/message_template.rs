// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.879373
//+ Revised: 	2026/11/03 04:54:15.879373
// -----------------------------------------------------------------------------

//! The text **Copy message** puts on the clipboard: a product's template with `{customer_name}`,
//! `{product_name}` and `{key}` filled in. Any other `{word}` is refused when the template is
//! saved, so a typo never reaches a customer as a literal placeholder.

pub const PLACEHOLDERS: [&str; 3] = ["customer_name", "product_name", "key"];

/// What a product gets when it is created without a template.
pub const DEFAULT: &str = "Hello {customer_name},\n\n\
Thank you for choosing {product_name}. Your licence key is:\n\n\
{key}\n\n\
Enter it when {product_name} asks for it. The key works on one computer at a time; if you move to \
a new computer, reply to this message and we'll free it for you.\n";

pub struct Values<'a> {
    pub customer_name: &'a str,
    pub product_name: &'a str,
    pub key: &'a str,
}

/// The first unknown placeholder in `template`, if any.
pub fn unknown_placeholder(template: &str) -> Option<String> {
    placeholders(template).find(|name| !PLACEHOLDERS.contains(name)).map(String::from)
}

pub fn fill(template: &str, values: &Values<'_>) -> String {
    let mut filled = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some((before, name, after)) = next_placeholder(rest) {
        filled.push_str(before);
        match name {
            "customer_name" => filled.push_str(values.customer_name),
            "product_name" => filled.push_str(values.product_name),
            "key" => filled.push_str(values.key),
            other => {
                filled.push('{');
                filled.push_str(other);
                filled.push('}');
            }
        }
        rest = after;
    }
    filled.push_str(rest);
    filled
}

fn placeholders(template: &str) -> impl Iterator<Item = &str> {
    let mut rest = template;
    std::iter::from_fn(move || {
        let (_, name, after) = next_placeholder(rest)?;
        rest = after;
        Some(name)
    })
}

/// Splits at the next `{word}` (letters, digits and `_`); braces around anything else are text.
fn next_placeholder(text: &str) -> Option<(&str, &str, &str)> {
    let mut search_from = 0;
    loop {
        let open = search_from + text[search_from..].find('{')?;
        let close = open + text[open..].find('}')?;
        let name = &text[open + 1..close];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some((&text[..open], name, &text[close + 1..]));
        }
        search_from = open + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: Values<'static> = Values { customer_name: "Ana", product_name: "Avalio Lite", key: "BRN-1" };

    #[test]
    fn placeholders_are_filled_everywhere_they_appear() {
        let filled = fill("Hi {customer_name}! {key} for {product_name}, {key}.", &VALUES);
        assert_eq!(filled, "Hi Ana! BRN-1 for Avalio Lite, BRN-1.");
    }

    #[test]
    fn braces_that_are_not_placeholders_stay_as_written() {
        assert_eq!(fill("{ } {a b} {} {{key}} }{", &VALUES), "{ } {a b} {} {BRN-1} }{");
        assert_eq!(unknown_placeholder("{ } {a b} {} }{"), None);
    }

    #[test]
    fn values_are_never_read_as_placeholders() {
        let tricky = Values { customer_name: "{key}", ..VALUES };
        assert_eq!(fill("{customer_name} {key}", &tricky), "{key} BRN-1");
    }

    #[test]
    fn unknown_placeholders_are_found() {
        assert_eq!(unknown_placeholder("Dear {customer_nme}, {key}").as_deref(), Some("customer_nme"));
        assert_eq!(unknown_placeholder(DEFAULT), None);
        assert_eq!(fill("{other}", &VALUES), "{other}");
    }

    #[test]
    fn the_default_template_uses_every_placeholder() {
        for name in PLACEHOLDERS {
            assert!(DEFAULT.contains(&format!("{{{name}}}")), "{name}");
        }
    }
}
