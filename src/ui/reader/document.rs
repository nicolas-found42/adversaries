//! Read-only structure from the same Markdown passed to the shared visual reader.
//! This is presentation data, never a second application or execution model.
use egui::{Context, Id};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Document {
    blocks: Vec<Node>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct Node {
    kind: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    href: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    anchor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    align: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<Node>,
}

impl Node {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }
    fn text(kind: &'static str, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            ..Self::default()
        }
    }
}

fn alignment(value: Alignment) -> &'static str {
    match value {
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::None => "start",
    }
}

fn block(tag: &Tag<'_>) -> Option<Node> {
    Some(match tag {
        Tag::Paragraph => Node::new("paragraph"),
        Tag::Heading { level, id, .. } => Node {
            level: Some(*level as u8),
            anchor: id.as_ref().map_or_else(String::new, ToString::to_string),
            ..Node::new("heading")
        },
        Tag::BlockQuote(_) => Node::new("quote"),
        Tag::CodeBlock(kind) => Node {
            title: match kind {
                CodeBlockKind::Fenced(info) => info.to_string(),
                CodeBlockKind::Indented => String::new(),
            },
            ..Node::new("code_block")
        },
        Tag::List(start) => Node {
            start: *start,
            ..Node::new("list")
        },
        Tag::Item => Node::new("item"),
        Tag::Table(values) => Node {
            align: values.iter().copied().map(alignment).collect(),
            ..Node::new("table")
        },
        _ => return None,
    })
}

fn inline(tag: &Tag<'_>) -> Option<Node> {
    Some(match tag {
        Tag::Emphasis => Node::new("emphasis"),
        Tag::Strong => Node::new("strong"),
        Tag::Strikethrough => Node::new("strike"),
        Tag::Link {
            dest_url, title, ..
        } => Node {
            href: dest_url.to_string(),
            title: title.to_string(),
            ..Node::new("link")
        },
        Tag::Image {
            dest_url, title, ..
        } => Node {
            href: dest_url.to_string(),
            title: title.to_string(),
            ..Node::new("image")
        },
        Tag::Superscript => Node::new("superscript"),
        Tag::Subscript => Node::new("subscript"),
        _ => return None,
    })
}

fn other(tag: &Tag<'_>) -> Node {
    match tag {
        Tag::TableHead => Node::new("table_head"),
        Tag::TableRow => Node::new("row"),
        Tag::TableCell => Node::new("cell"),
        Tag::FootnoteDefinition(label) => Node {
            anchor: format!("footnote-{label}"),
            title: label.to_string(),
            ..Node::new("footnote")
        },
        Tag::DefinitionList => Node::new("definition_list"),
        Tag::DefinitionListTitle => Node::new("definition_title"),
        Tag::DefinitionListDefinition => Node::new("definition"),
        Tag::HtmlBlock | Tag::MetadataBlock(_) => Node::new("literal_block"),
        _ => Node::new("span"),
    }
}

fn leaf(event: Event<'_>) -> Option<Node> {
    Some(match event {
        Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
            Node::text("text", text.to_string())
        }
        Event::Code(text) => Node::text("code", text.to_string()),
        Event::SoftBreak => Node::text("text", " "),
        Event::HardBreak => Node::new("break"),
        Event::Rule => Node::new("rule"),
        Event::TaskListMarker(checked) => Node {
            checked: Some(checked),
            ..Node::new("task")
        },
        Event::FootnoteReference(label) => Node {
            href: format!("#footnote-{label}"),
            ..Node::text("footnote_reference", label.to_string())
        },
        Event::InlineMath(text) | Event::DisplayMath(text) => Node::text("code", text.to_string()),
        _ => return None,
    })
}

fn append(parent: &mut Node, node: Node) {
    if node.kind == "text"
        && let Some(previous) = parent.children.last_mut()
        && previous.kind == "text"
    {
        previous.text.push_str(&node.text);
    } else {
        parent.children.push(node);
    }
}

fn parse(source: &str) -> Document {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_DEFINITION_LIST;
    let mut stack = vec![Node::new("document")];
    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(tag) => {
                let mut node = block(&tag)
                    .or_else(|| inline(&tag))
                    .unwrap_or_else(|| other(&tag));
                if node.kind == "cell"
                    && stack
                        .last()
                        .is_some_and(|parent| parent.kind == "table_head")
                {
                    node.kind = "header_cell";
                }
                stack.push(node);
            }
            Event::End(_) => {
                let node = stack.pop().expect("balanced Markdown event stream");
                append(stack.last_mut().expect("document root remains"), node);
            }
            event => {
                if let Some(node) = leaf(event) {
                    append(stack.last_mut().expect("document root remains"), node);
                }
            }
        }
    }
    Document {
        blocks: stack.pop().expect("document root").children,
    }
}

#[derive(Clone, Default)]
struct Registry {
    frame: HashMap<u64, Arc<Document>>,
    cache: VecDeque<(String, Arc<Document>)>,
}

fn registry_id() -> Id {
    Id::unique("reader_document_registry")
}

pub(crate) fn begin_frame(ctx: &Context) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Registry>(registry_id())
            .frame
            .clear();
    });
}

pub(super) fn register(ctx: &Context, id: Id, text: &str) {
    if !ctx.has_accesskit_node(egui::accesskit_root_id()) {
        return;
    }
    ctx.data_mut(|data| {
        let registry = data.get_temp_mut_or_default::<Registry>(registry_id());
        let document =
            if let Some(index) = registry.cache.iter().position(|(source, _)| source == text) {
                let entry = registry.cache.remove(index).unwrap();
                let document = Arc::clone(&entry.1);
                registry.cache.push_front(entry);
                document
            } else {
                let document = Arc::new(parse(text));
                registry
                    .cache
                    .push_front((text.to_owned(), Arc::clone(&document)));
                document
            };
        while registry.cache.len() > 8 {
            registry.cache.pop_back();
        }
        registry.frame.insert(id.value(), document);
    });
}

pub(crate) fn projection(ctx: &Context, id: u64) -> Option<Document> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Registry>(registry_id())
            .frame
            .get(&id)
            .map(|document| document.as_ref().clone())
    })
}

#[cfg(test)]
mod tests {
    use super::{Node, parse};

    fn text(node: &Node) -> String {
        node.text.clone() + &node.children.iter().map(text).collect::<String>()
    }

    #[test]
    fn inline_markers_stay_in_one_paragraph_without_added_spaces() {
        let source = "C1: [SOURCE EXCERPT MATCHED] A **bold**, *qualified* claim with `x < 2` and [evidence](https://example.test).";
        let document = parse(source);
        assert_eq!(document.blocks.len(), 1);
        let paragraph = &document.blocks[0];
        assert_eq!(paragraph.kind, "paragraph");
        assert_eq!(
            text(paragraph),
            "C1: [SOURCE EXCERPT MATCHED] A bold, qualified claim with x < 2 and evidence."
        );
        assert_eq!(paragraph.children[1].kind, "strong");
        assert_eq!(paragraph.children.last().unwrap().text, ".");
    }

    #[test]
    fn references_resolve_across_tables_without_splitting_the_source() {
        let document = parse(
            "Read [the evidence][ref].\n\n| Role | Claim |\n| :-- | --: |\n| Opening | **Original** |\n\n[ref]: https://example.test/evidence \"Source\"\n\nFinal paragraph.",
        );
        let link = &document.blocks[0].children[1];
        assert_eq!(link.href, "https://example.test/evidence");
        assert_eq!(link.title, "Source");
        let table = &document.blocks[1];
        assert_eq!(table.align, ["left", "right"]);
        assert_eq!(table.children[0].children[0].kind, "header_cell");
        assert_eq!(text(document.blocks.last().unwrap()), "Final paragraph.");
    }

    #[test]
    fn lists_code_breaks_and_footnotes_preserve_structure() {
        let source = "## Details\n\n7. First\n8. Second\n\n- [x] Done\n- [ ] Pending\n\n```rust\n  let a = 1;\n\tlet b = 2;\n```\n\nOne  \ntwo[^n].\n\n[^n]: Footnote text.";
        let document = parse(source);
        assert_eq!(document.blocks[0].level, Some(2));
        assert_eq!(document.blocks[1].start, Some(7));
        assert_eq!(
            document.blocks[2].children[0].children[0].checked,
            Some(true)
        );
        assert_eq!(text(&document.blocks[3]), "  let a = 1;\n\tlet b = 2;\n");
        assert_eq!(document.blocks[4].children[1].kind, "break");
        assert_eq!(document.blocks[5].anchor, "footnote-n");
    }

    #[test]
    fn raw_html_is_literal_text_data_and_not_a_render_instruction() {
        let source = "<script>window.executed = true;</script>\n\nA <b>literal</b> tag.";
        let document = parse(source);
        assert_eq!(document.blocks[0].kind, "literal_block");
        assert_eq!(
            text(&document.blocks[0]),
            "<script>window.executed = true;</script>\n"
        );
        assert_eq!(text(&document.blocks[1]), "A <b>literal</b> tag.");
    }
}
