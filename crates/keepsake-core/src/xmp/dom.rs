//! A small namespace-aware XML tree for XMP packets.
//!
//! The tree keeps everything it does not understand (other tools' properties,
//! comments, processing instructions) so a read-modify-write cycle preserves
//! foreign metadata. Whitespace-only text between elements is dropped and the
//! output is re-indented.

use std::collections::HashMap;
use std::fmt::Write as _;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::XmpError;

pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// An attribute with its namespace resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// Qualified name as written (e.g. `xmp:Rating`).
    pub name: String,
    /// Resolved namespace URI (`None` for unprefixed attributes and `xmlns` declarations).
    pub ns: Option<String>,
    pub local: String,
    /// Unescaped value.
    pub value: String,
}

impl Attr {
    pub fn is_ns_decl(&self) -> bool {
        self.name == "xmlns" || self.name.starts_with("xmlns:")
    }

    pub fn is(&self, ns: &str, local: &str) -> bool {
        self.ns.as_deref() == Some(ns) && self.local == local
    }
}

/// A child node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
}

/// An element with its namespace resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub ns: Option<String>,
    pub local: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
}

impl Element {
    /// A new element; `prefix` must be declared in scope by the caller.
    pub fn new(prefix: &str, ns: &str, local: &str) -> Self {
        Self {
            name: format!("{prefix}:{local}"),
            ns: Some(ns.to_owned()),
            local: local.to_owned(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn is(&self, ns: &str, local: &str) -> bool {
        self.ns.as_deref() == Some(ns) && self.local == local
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.children.iter_mut().filter_map(|c| match c {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn child(&self, ns: &str, local: &str) -> Option<&Element> {
        self.elements().find(|e| e.is(ns, local))
    }

    pub fn attr(&self, ns: &str, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.is(ns, local))
            .map(|a| a.value.as_str())
    }

    /// Concatenated text content of direct text children.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for c in &self.children {
            if let Node::Text(t) = c {
                s.push_str(t);
            }
        }
        s
    }

    pub fn push_element(&mut self, e: Element) {
        self.children.push(Node::Element(e));
    }

    pub fn push_text(&mut self, t: &str) {
        self.children.push(Node::Text(t.to_owned()));
    }

    pub fn set_attr(&mut self, name: &str, ns: Option<&str>, local: &str, value: &str) {
        if let Some(a) = self.attrs.iter_mut().find(|a| a.name == name) {
            a.value = value.to_owned();
            return;
        }
        self.attrs.push(Attr {
            name: name.to_owned(),
            ns: ns.map(str::to_owned),
            local: local.to_owned(),
            value: value.to_owned(),
        });
    }

    /// The URI bound to `prefix` by a declaration on this element.
    pub fn declared_ns(&self, prefix: &str) -> Option<&str> {
        let key = format!("xmlns:{prefix}");
        self.attrs
            .iter()
            .find(|a| a.name == key)
            .map(|a| a.value.as_str())
    }
}

/// A parsed XML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Processing instructions and comments before the root (raw, without `<?`/`?>`).
    pub prolog: Vec<Misc>,
    pub root: Element,
    pub epilog: Vec<Misc>,
}

/// Content outside the root element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Misc {
    /// Processing instruction body, e.g. `xpacket end="w"`.
    Pi(String),
    Comment(String),
}

type Scope = HashMap<String, String>;

fn resolve(scopes: &[Scope], prefix: Option<&str>, is_attr: bool) -> Option<String> {
    match prefix {
        Some("xml") => Some(XML_NS.to_owned()),
        Some(p) => scopes.iter().rev().find_map(|s| s.get(p).cloned()),
        // Unprefixed attributes are in no namespace; unprefixed elements use the default.
        None if is_attr => None,
        None => scopes
            .iter()
            .rev()
            .find_map(|s| s.get("").cloned())
            .filter(|u| !u.is_empty()),
    }
}

fn split_qname(q: &str) -> (Option<&str>, &str) {
    match q.split_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, q),
    }
}

fn build_element(
    start: &quick_xml::events::BytesStart<'_>,
    scopes: &mut Vec<Scope>,
) -> Result<Element, XmpError> {
    let mut raw_attrs = Vec::new();
    let mut scope = Scope::new();
    for a in start.attributes() {
        let a = a.map_err(|e| XmpError::Parse(e.to_string()))?;
        let name = a.key.0.to_owned();
        let value = quick_xml::escape::unescape(&a.value)
            .map_err(|e| XmpError::Parse(e.to_string()))?
            .into_owned();
        if name == "xmlns" {
            scope.insert(String::new(), value.clone());
        } else if let Some(p) = name.strip_prefix("xmlns:") {
            scope.insert(p.to_owned(), value.clone());
        }
        raw_attrs.push((name, value));
    }
    scopes.push(scope);

    let qname = start.name().0.to_owned();
    let (prefix, local) = split_qname(&qname);
    if prefix.is_some() && resolve(scopes, prefix, false).is_none() {
        return Err(XmpError::Parse(format!("undeclared prefix in <{qname}>")));
    }
    let ns = resolve(scopes, prefix, false);
    let local = local.to_owned();

    let mut attrs = Vec::with_capacity(raw_attrs.len());
    for (name, value) in raw_attrs {
        let is_decl = name == "xmlns" || name.starts_with("xmlns:");
        let (p, l) = split_qname(&name);
        let ns = if is_decl {
            None
        } else {
            resolve(scopes, p, true)
        };
        let local = l.to_owned();
        attrs.push(Attr {
            name,
            ns,
            local,
            value,
        });
    }
    Ok(Element {
        name: qname,
        ns,
        local,
        attrs,
        children: Vec::new(),
    })
}

fn append_text(stack: &mut [Element], text: &str) -> Result<(), XmpError> {
    match stack.last_mut() {
        Some(el) => {
            if let Some(Node::Text(t)) = el.children.last_mut() {
                t.push_str(text);
            } else {
                el.children.push(Node::Text(text.to_owned()));
            }
            Ok(())
        }
        None if text.trim().is_empty() => Ok(()),
        None => Err(XmpError::Parse("text outside the root element".into())),
    }
}

fn resolve_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => return None,
    })
}

/// Drop whitespace-only text from elements that contain child elements.
fn strip_layout_whitespace(el: &mut Element) {
    let has_elements = el.children.iter().any(|c| matches!(c, Node::Element(_)));
    if has_elements {
        el.children
            .retain(|c| !matches!(c, Node::Text(t) if t.trim().is_empty()));
    }
    for c in el.elements_mut() {
        strip_layout_whitespace(c);
    }
}

/// Parse an XML document.
pub fn parse(input: &str) -> Result<Document, XmpError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut reader = Reader::from_str(input);
    let mut scopes: Vec<Scope> = Vec::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut prolog = Vec::new();
    let mut epilog = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        let ev = reader
            .read_event()
            .map_err(|e| XmpError::Parse(format!("at byte {}: {e}", reader.buffer_position())))?;
        match ev {
            Event::Start(s) => {
                if root.is_some() {
                    return Err(XmpError::Parse("more than one root element".into()));
                }
                let el = build_element(&s, &mut scopes)?;
                stack.push(el);
            }
            Event::Empty(s) => {
                if root.is_some() {
                    return Err(XmpError::Parse("more than one root element".into()));
                }
                let el = build_element(&s, &mut scopes)?;
                scopes.pop();
                match stack.last_mut() {
                    Some(parent) => parent.push_element(el),
                    None => root = Some(el),
                }
            }
            Event::End(_) => {
                scopes.pop();
                let el = stack
                    .pop()
                    .ok_or_else(|| XmpError::Parse("unbalanced end tag".into()))?;
                match stack.last_mut() {
                    Some(parent) => parent.push_element(el),
                    None => root = Some(el),
                }
            }
            Event::Text(t) => append_text(&mut stack, &t.xml10_content())?,
            Event::CData(t) => {
                let s = t.into_inner().into_owned();
                append_text(&mut stack, &s)?
            }
            Event::GeneralRef(r) => {
                let s = if r.is_char_ref() {
                    r.resolve_char_ref()
                        .map_err(|e| XmpError::Parse(e.to_string()))?
                        .map(String::from)
                        .ok_or_else(|| XmpError::Parse("invalid character reference".into()))?
                } else {
                    let name = r.xml10_content();
                    resolve_entity(&name)
                        .map(str::to_owned)
                        .ok_or_else(|| XmpError::Parse(format!("unknown entity &{name};")))?
                };
                append_text(&mut stack, &s)?
            }
            Event::Comment(c) => {
                let c = c.xml10_content().into_owned();
                match (stack.last_mut(), &root) {
                    (Some(el), _) => el.children.push(Node::Comment(c)),
                    (None, None) => prolog.push(Misc::Comment(c)),
                    (None, Some(_)) => epilog.push(Misc::Comment(c)),
                }
            }
            Event::PI(p) => {
                let body = p.into_inner().into_owned();
                if stack.is_empty() {
                    if root.is_none() {
                        prolog.push(Misc::Pi(body));
                    } else {
                        epilog.push(Misc::Pi(body));
                    }
                }
                // Processing instructions inside the tree are dropped (not used by XMP).
            }
            Event::Decl(_) => {}
            Event::DocType(_) => {
                return Err(XmpError::Parse("DOCTYPE is not allowed in XMP".into()))
            }
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err(XmpError::Parse("unexpected end of document".into()));
    }
    let mut root = root.ok_or_else(|| XmpError::Parse("no root element".into()))?;
    strip_layout_whitespace(&mut root);
    Ok(Document {
        prolog,
        root,
        epilog,
    })
}

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            '\t' => out.push_str("&#x9;"),
            _ => out.push(c),
        }
    }
}

fn write_element(el: &Element, depth: usize, out: &mut String) {
    let indent = " ".repeat(depth);
    out.push_str(&indent);
    out.push('<');
    out.push_str(&el.name);
    for a in &el.attrs {
        if el.attrs.len() > 2 {
            out.push('\n');
            out.push_str(&indent);
            out.push_str("    ");
        } else {
            out.push(' ');
        }
        out.push_str(&a.name);
        out.push_str("=\"");
        escape_attr(&a.value, out);
        out.push('"');
    }
    if el.children.is_empty() {
        out.push_str("/>\n");
        return;
    }
    let only_text = el.children.iter().all(|c| matches!(c, Node::Text(_)));
    if only_text {
        out.push('>');
        escape_text(&el.text(), out);
    } else {
        out.push_str(">\n");
        for c in &el.children {
            match c {
                Node::Element(e) => write_element(e, depth + 1, out),
                Node::Text(t) => {
                    out.push_str(&indent);
                    out.push(' ');
                    escape_text(t, out);
                    out.push('\n');
                }
                Node::Comment(c) => {
                    let _ = writeln!(out, "{indent} <!--{c}-->");
                }
            }
        }
        out.push_str(&indent);
    }
    out.push_str("</");
    out.push_str(&el.name);
    out.push_str(">\n");
}

fn write_misc(m: &Misc, out: &mut String) {
    match m {
        Misc::Pi(p) => {
            let _ = writeln!(out, "<?{p}?>");
        }
        Misc::Comment(c) => {
            let _ = writeln!(out, "<!--{c}-->");
        }
    }
}

/// Serialise a document as UTF-8 text.
pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    for m in &doc.prolog {
        write_misc(m, &mut out);
    }
    write_element(&doc.root, 0, &mut out);
    for m in &doc.epilog {
        write_misc(m, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

    #[test]
    fn round_trips_entities_and_namespaces() {
        let src = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" a="1 &amp; 2">
      <dc:title>Fish &amp; chips &#x263A;</dc:title>
      <!-- keep me -->
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;
        let doc = parse(src).unwrap();
        let rdf = doc.root.child(RDF, "RDF").unwrap();
        let desc = rdf.child(RDF, "Description").unwrap();
        assert_eq!(
            desc.attrs.iter().find(|a| a.name == "a").unwrap().value,
            "1 & 2"
        );
        let title = desc
            .child("http://purl.org/dc/elements/1.1/", "title")
            .unwrap();
        assert_eq!(title.text(), "Fish & chips \u{263A}");
        let again = parse(&serialize(&doc)).unwrap();
        assert_eq!(again, doc);
        assert_eq!(doc.prolog.len(), 1);
        assert_eq!(doc.epilog, vec![Misc::Pi("xpacket end=\"w\"".into())]);
    }

    #[test]
    fn rejects_doctype_and_undeclared_prefix() {
        assert!(parse("<!DOCTYPE x><x/>").is_err());
        assert!(parse("<a:b/>").is_err());
    }

    #[test]
    fn preserves_newlines_in_attributes() {
        let mut doc = parse("<r/>").unwrap();
        doc.root.set_attr("v", None, "v", "line1\nline2\t\"q\"");
        let again = parse(&serialize(&doc)).unwrap();
        assert_eq!(again.root.attrs[0].value, "line1\nline2\t\"q\"");
    }
}
