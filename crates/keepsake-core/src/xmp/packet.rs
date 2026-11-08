//! Generic XMP property access over the [`dom`](super::dom) tree.

use super::dom::{self, Attr, Document, Element, Misc, Node};
use super::{ns, XmpError};

/// XMP array container type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    Bag,
    Seq,
    Alt,
}

impl ArrayKind {
    fn local(self) -> &'static str {
        match self {
            Self::Bag => "Bag",
            Self::Seq => "Seq",
            Self::Alt => "Alt",
        }
    }
}

/// A struct field: `(namespace, local name, value)`.
pub type Field = (&'static str, &'static str, String);

/// One field read from a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadField {
    pub ns: Option<String>,
    pub local: String,
    pub value: String,
}

/// A property value of any shape, nested as deep as it goes: text, a
/// struct of named fields, or an array (MWG face regions are a struct with
/// an array of structs that hold a struct).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    /// `(namespace, local name, value)`, in order.
    Struct(Vec<(String, String, Value)>),
    Array(ArrayKind, Vec<Value>),
}

impl Value {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// A struct from `(namespace, local name, value)` fields.
    pub fn fields(f: impl IntoIterator<Item = (&'static str, &'static str, Value)>) -> Self {
        Self::Struct(
            f.into_iter()
                .map(|(n, l, v)| (n.to_owned(), l.to_owned(), v))
                .collect(),
        )
    }

    /// A struct's field.
    pub fn field(&self, ns: &str, local: &str) -> Option<&Value> {
        match self {
            Self::Struct(f) => f
                .iter()
                .find(|(n, l, _)| n == ns && l == local)
                .map(|(_, _, v)| v),
            _ => None,
        }
    }

    /// The text, if it is text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    /// An array's items (none for text or a struct).
    pub fn items(&self) -> &[Value] {
        match self {
            Self::Array(_, v) => v,
            _ => &[],
        }
    }
}

fn kind_of(e: &Element) -> Option<ArrayKind> {
    [ArrayKind::Bag, ArrayKind::Seq, ArrayKind::Alt]
        .into_iter()
        .find(|k| is_rdf(e, k.local()))
}

/// A property element's value, in any of the RDF forms: an array, a
/// resource, a struct (`rdf:parseType="Resource"`, an `rdf:Description`, or
/// fields as attributes or child elements) or text.
fn read_value(e: &Element) -> Value {
    if let Some(arr) = e.elements().find(|c| kind_of(c).is_some()) {
        let kind = kind_of(arr).unwrap_or(ArrayKind::Bag);
        return Value::Array(
            kind,
            arr.elements()
                .filter(|li| is_rdf(li, "li"))
                .map(read_value)
                .collect(),
        );
    }
    if let Some(r) = e.attr(ns::RDF, "resource") {
        return Value::Text(r.to_owned());
    }
    let src = e.child(ns::RDF, "Description").unwrap_or(e);
    let mut fields = Vec::new();
    for a in &src.attrs {
        let ours =
            !a.is_ns_decl() && !is_rdf_attr(a) && a.ns.as_deref().is_some_and(|n| n != dom::XML_NS);
        if ours {
            if let Some(n) = &a.ns {
                fields.push((n.clone(), a.local.clone(), Value::Text(a.value.clone())));
            }
        }
    }
    for c in src.elements() {
        if let Some(n) = c.ns.as_deref().filter(|n| *n != ns::RDF) {
            fields.push((n.to_owned(), c.local.clone(), read_value(c)));
        }
    }
    let structured = !fields.is_empty() || e.attr(ns::RDF, "parseType") == Some("Resource");
    if structured {
        Value::Struct(fields)
    } else {
        Value::Text(e.text())
    }
}

/// Look a field up by namespace and local name.
pub fn field<'a>(fields: &'a [ReadField], ns: &str, local: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|f| f.ns.as_deref() == Some(ns) && f.local == local)
        .map(|f| f.value.as_str())
}

/// An XMP packet that preserves properties it does not manage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmpPacket {
    doc: Document,
}

fn is_rdf(e: &Element, local: &str) -> bool {
    e.is(ns::RDF, local)
}

fn is_rdf_attr(a: &Attr) -> bool {
    a.ns.as_deref() == Some(ns::RDF)
}

/// Simple value of a property element (text or `rdf:resource`).
fn simple_value(e: &Element) -> String {
    e.attr(ns::RDF, "resource")
        .map(str::to_owned)
        .unwrap_or_else(|| e.text())
}

/// Fields of a struct-valued element in any of the RDF serialisations.
fn struct_fields(e: &Element) -> Vec<ReadField> {
    let mut out = Vec::new();
    let collect = |src: &Element, out: &mut Vec<ReadField>| {
        for a in &src.attrs {
            if !a.is_ns_decl() && !is_rdf_attr(a) && a.ns.is_some() {
                out.push(ReadField {
                    ns: a.ns.clone(),
                    local: a.local.clone(),
                    value: a.value.clone(),
                });
            }
        }
        for c in src.elements() {
            if !c.ns.as_deref().is_some_and(|n| n == ns::RDF) {
                out.push(ReadField {
                    ns: c.ns.clone(),
                    local: c.local.clone(),
                    value: simple_value(c),
                });
            }
        }
    };
    match e.child(ns::RDF, "Description") {
        Some(d) => collect(d, &mut out),
        None => collect(e, &mut out),
    }
    out
}

fn array_items(e: &Element) -> Option<impl Iterator<Item = &Element>> {
    e.elements()
        .find(|c| is_rdf(c, "Bag") || is_rdf(c, "Seq") || is_rdf(c, "Alt"))
        .map(|arr| arr.elements().filter(|li| is_rdf(li, "li")))
}

impl Default for XmpPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl XmpPacket {
    /// An empty packet with one `rdf:Description`.
    pub fn new() -> Self {
        let mut root = Element::new("x", ns::X, "xmpmeta");
        root.set_attr("xmlns:x", None, "x", ns::X);
        root.set_attr(
            "x:xmptk",
            Some(ns::X),
            "xmptk",
            &format!("Keepsake {}", crate::version()),
        );
        let mut rdf = Element::new("rdf", ns::RDF, "RDF");
        rdf.set_attr("xmlns:rdf", None, "rdf", ns::RDF);
        let mut desc = Element::new("rdf", ns::RDF, "Description");
        desc.set_attr("rdf:about", Some(ns::RDF), "about", "");
        rdf.push_element(desc);
        root.push_element(rdf);
        Self {
            doc: Document {
                prolog: vec![Misc::Pi(
                    "xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"".into(),
                )],
                root,
                epilog: vec![Misc::Pi("xpacket end=\"w\"".into())],
            },
        }
    }

    pub fn parse(xml: &str) -> Result<Self, XmpError> {
        let doc = dom::parse(xml)?;
        let ok = is_rdf(&doc.root, "RDF") || doc.root.child(ns::RDF, "RDF").is_some();
        if !ok {
            return Err(XmpError::Parse("no rdf:RDF element".into()));
        }
        Ok(Self { doc })
    }

    pub fn to_xml(&self) -> String {
        dom::serialize(&self.doc)
    }

    fn rdf(&self) -> Option<&Element> {
        if is_rdf(&self.doc.root, "RDF") {
            Some(&self.doc.root)
        } else {
            self.doc.root.child(ns::RDF, "RDF")
        }
    }

    fn rdf_mut(&mut self) -> Option<&mut Element> {
        if is_rdf(&self.doc.root, "RDF") {
            Some(&mut self.doc.root)
        } else {
            self.doc.root.elements_mut().find(|e| is_rdf(e, "RDF"))
        }
    }

    fn descriptions(&self) -> impl Iterator<Item = &Element> {
        self.rdf()
            .into_iter()
            .flat_map(|r| r.elements().filter(|e| is_rdf(e, "Description")))
    }

    /// The property element `ns:local`, from any description.
    pub fn property(&self, ns: &str, local: &str) -> Option<&Element> {
        self.descriptions().find_map(|d| d.child(ns, local))
    }

    /// A simple property in attribute or element form.
    pub fn simple(&self, ns: &str, local: &str) -> Option<String> {
        for d in self.descriptions() {
            if let Some(v) = d.attr(ns, local) {
                return Some(v.to_owned());
            }
            if let Some(e) = d.child(ns, local) {
                return Some(simple_value(e));
            }
        }
        None
    }

    /// Items of an array property (empty when absent).
    pub fn array(&self, ns: &str, local: &str) -> Vec<String> {
        match self.property(ns, local).and_then(array_items) {
            Some(items) => items.map(simple_value).collect(),
            None => Vec::new(),
        }
    }

    /// The `x-default` (or first) entry of a language alternative.
    pub fn lang_alt_default(&self, ns: &str, local: &str) -> Option<String> {
        let prop = self.property(ns, local)?;
        let Some(items) = array_items(prop) else {
            // Some writers store a plain string.
            return Some(simple_value(prop));
        };
        let items: Vec<&Element> = items.collect();
        items
            .iter()
            .find(|li| li.attr(dom::XML_NS, "lang") == Some("x-default"))
            .or_else(|| items.first())
            .map(|li| li.text())
    }

    /// A property's value, whatever its shape (see [`Value`]).
    pub fn value(&self, ns: &str, local: &str) -> Option<Value> {
        self.property(ns, local).map(read_value)
    }

    /// Fields of a struct property.
    pub fn structure(&self, ns: &str, local: &str) -> Option<Vec<ReadField>> {
        self.property(ns, local).map(struct_fields)
    }

    /// Fields of each struct in an array-of-structs property.
    pub fn struct_array(&self, ns: &str, local: &str) -> Vec<Vec<ReadField>> {
        match self.property(ns, local).and_then(array_items) {
            Some(items) => items.map(struct_fields).collect(),
            None => Vec::new(),
        }
    }

    /// Remove a property (attribute and element forms) from every description.
    pub fn remove(&mut self, ns: &str, local: &str) {
        if let Some(rdf) = self.rdf_mut() {
            for d in rdf.elements_mut().filter(|e| is_rdf(e, "Description")) {
                d.attrs.retain(|a| !a.is(ns, local));
                d.children
                    .retain(|c| !matches!(c, Node::Element(e) if e.is(ns, local)));
            }
        }
    }

    /// Prefix bound to `uri` in scope of the first description, declaring
    /// `preferred` (or a numbered variant) there if needed.
    fn ensure_prefix(&mut self, preferred: &str, uri: &str) -> String {
        let mut taken: Vec<(String, String)> = Vec::new();
        let mut gather = |e: &Element| {
            for a in &e.attrs {
                if let Some(p) = a.name.strip_prefix("xmlns:") {
                    taken.push((p.to_owned(), a.value.clone()));
                }
            }
        };
        gather(&self.doc.root);
        if let Some(rdf) = self.rdf() {
            gather(rdf);
            if let Some(d) = rdf.elements().find(|e| is_rdf(e, "Description")) {
                gather(d);
            }
        }
        if let Some((p, _)) = taken.iter().find(|(_, u)| u == uri) {
            return p.clone();
        }
        let mut prefix = preferred.to_owned();
        let mut n = 1;
        while taken.iter().any(|(p, _)| *p == prefix) {
            prefix = format!("{preferred}{n}");
            n += 1;
        }
        if let Some(d) = self.first_description_mut() {
            d.set_attr(&format!("xmlns:{prefix}"), None, &prefix, uri);
        }
        prefix
    }

    fn first_description_mut(&mut self) -> Option<&mut Element> {
        let rdf = self.rdf_mut()?;
        if !rdf.elements().any(|e| is_rdf(e, "Description")) {
            let mut desc = Element::new(
                rdf.name.split(':').next().unwrap_or("rdf"),
                ns::RDF,
                "Description",
            );
            let rdf_prefix = desc.name.split(':').next().unwrap_or("rdf").to_owned();
            desc.set_attr(&format!("{rdf_prefix}:about"), Some(ns::RDF), "about", "");
            rdf.push_element(desc);
        }
        rdf.elements_mut().find(|e| is_rdf(e, "Description"))
    }

    fn rdf_prefix(&mut self) -> String {
        self.ensure_prefix("rdf", ns::RDF)
    }

    fn new_prop(&mut self, uri: &str, local: &str) -> Element {
        let prefix = self.ensure_prefix(ns::preferred_prefix(uri), uri);
        Element::new(&prefix, uri, local)
    }

    fn push_prop(&mut self, el: Element) {
        if let Some(d) = self.first_description_mut() {
            d.push_element(el);
        }
    }

    /// Replace a simple property.
    pub fn set_simple(&mut self, uri: &str, local: &str, value: &str) {
        self.remove(uri, local);
        let mut el = self.new_prop(uri, local);
        el.push_text(value);
        self.push_prop(el);
    }

    fn make_array(&mut self, kind: ArrayKind, items: Vec<Element>) -> Element {
        let rp = self.rdf_prefix();
        let mut arr = Element::new(&rp, ns::RDF, kind.local());
        for li in items {
            arr.push_element(li);
        }
        arr
    }

    fn li(&mut self) -> Element {
        let rp = self.rdf_prefix();
        Element::new(&rp, ns::RDF, "li")
    }

    /// Replace an array property; an empty list removes it.
    pub fn set_array(&mut self, uri: &str, local: &str, kind: ArrayKind, items: &[String]) {
        self.remove(uri, local);
        if items.is_empty() {
            return;
        }
        let mut lis = Vec::new();
        for v in items {
            let mut li = self.li();
            li.push_text(v);
            lis.push(li);
        }
        let mut el = self.new_prop(uri, local);
        let arr = self.make_array(kind, lis);
        el.push_element(arr);
        self.push_prop(el);
    }

    /// Set (or with `None` remove) the `x-default` entry of a language
    /// alternative, keeping other languages.
    pub fn set_lang_alt_default(&mut self, uri: &str, local: &str, value: Option<&str>) {
        let mut others: Vec<Element> = Vec::new();
        if let Some(prop) = self.property(uri, local) {
            if let Some(items) = array_items(prop) {
                others = items
                    .filter(|li| li.attr(dom::XML_NS, "lang") != Some("x-default"))
                    .cloned()
                    .collect();
            }
        }
        self.remove(uri, local);
        let mut lis = Vec::new();
        if let Some(v) = value {
            let mut li = self.li();
            li.set_attr("xml:lang", Some(dom::XML_NS), "lang", "x-default");
            li.push_text(v);
            lis.push(li);
        }
        lis.extend(others);
        if lis.is_empty() {
            return;
        }
        let mut el = self.new_prop(uri, local);
        let arr = self.make_array(ArrayKind::Alt, lis);
        el.push_element(arr);
        self.push_prop(el);
    }

    fn struct_element(&mut self, mut container: Element, fields: &[Field]) -> Element {
        let rp = self.rdf_prefix();
        container.set_attr(
            &format!("{rp}:parseType"),
            Some(ns::RDF),
            "parseType",
            "Resource",
        );
        for (fns, flocal, value) in fields {
            let mut f = self.new_prop(fns, flocal);
            f.push_text(value);
            container.push_element(f);
        }
        container
    }

    /// `el` with `v` as its content.
    fn build(&mut self, mut el: Element, v: &Value) -> Element {
        match v {
            Value::Text(t) => el.push_text(t),
            Value::Array(kind, items) => {
                let mut lis = Vec::with_capacity(items.len());
                for it in items {
                    let li = self.li();
                    lis.push(self.build(li, it));
                }
                let arr = self.make_array(*kind, lis);
                el.push_element(arr);
            }
            Value::Struct(fields) => {
                let rp = self.rdf_prefix();
                el.set_attr(
                    &format!("{rp}:parseType"),
                    Some(ns::RDF),
                    "parseType",
                    "Resource",
                );
                for (fns, flocal, fv) in fields {
                    let f = self.new_prop(fns, flocal);
                    let f = self.build(f, fv);
                    el.push_element(f);
                }
            }
        }
        el
    }

    /// Replace a property with a value of any shape; `None` removes it.
    pub fn set_value(&mut self, uri: &str, local: &str, v: Option<&Value>) {
        self.remove(uri, local);
        if let Some(v) = v {
            let el = self.new_prop(uri, local);
            let el = self.build(el, v);
            self.push_prop(el);
        }
    }

    /// Replace a struct property; `None` removes it.
    pub fn set_struct(&mut self, uri: &str, local: &str, fields: Option<&[Field]>) {
        self.remove(uri, local);
        if let Some(fields) = fields {
            let el = self.new_prop(uri, local);
            let el = self.struct_element(el, fields);
            self.push_prop(el);
        }
    }

    /// Replace an array of structs; an empty list removes it.
    pub fn set_struct_array(
        &mut self,
        uri: &str,
        local: &str,
        kind: ArrayKind,
        items: &[Vec<Field>],
    ) {
        self.remove(uri, local);
        if items.is_empty() {
            return;
        }
        let mut lis = Vec::new();
        for fields in items {
            let li = self.li();
            lis.push(self.struct_element(li, fields));
        }
        let mut el = self.new_prop(uri, local);
        let arr = self.make_array(kind, lis);
        el.push_element(arr);
        self.push_prop(el);
    }
}
