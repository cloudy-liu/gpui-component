use std::sync::{Arc, Mutex};

use super::{
    MarkdownExtensions, TextView, TextViewImageSource, TextViewState, TextViewStyle, format,
    node::{BlockNode, CodeBlock, NodeContext},
};
use crate::{
    ActiveTheme as _,
    highlighter::{HighlightTheme, ThemeStyle},
};
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div, px,
};

fn document(source: &str, alerts: bool) -> super::document::ParsedDocument {
    let mut context = NodeContext::default();
    if alerts {
        context.markdown_extensions = Arc::new(MarkdownExtensions::default().github_alerts());
    }
    format::markdown::parse(source, &mut context, &HighlightTheme::default_light()).unwrap()
}

#[test]
fn github_alerts_are_opt_in_and_require_an_unescaped_complete_first_line() {
    for kind in ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"] {
        let source = format!("> [!{kind}]\n> Body with **emphasis**.");
        let enabled = document(&source, true);
        assert!(
            matches!(&enabled.blocks[0], BlockNode::Blockquote { alert: Some(name), .. } if name == kind)
        );
        assert!(!enabled.text().contains("[!"));
        assert!(enabled.text().contains("Body with emphasis."));
        assert!(matches!(
            &document(&source, false).blocks[0],
            BlockNode::Blockquote { alert: None, .. }
        ));
    }
    for source in [
        "> \\[!NOTE]\n> literal",
        "> Text [!NOTE]\n> ordinary",
        "> [!NOTE\n> incomplete",
        "> [!NOTE] trailing text",
        "> `[!NOTE]`",
    ] {
        assert!(
            matches!(
                &document(source, true).blocks[0],
                BlockNode::Blockquote { alert: None, .. }
            ),
            "{source}"
        );
    }
    assert!(matches!(
        &document("```\n> [!NOTE]\n```", true).blocks[0],
        BlockNode::CodeBlock(_)
    ));
}

#[test]
fn soft_hard_and_html_breaks_keep_markdown_meaning() {
    let parsed = document("soft\nwrap  \nhard<br>html", true);
    assert_eq!(parsed.text().trim(), "soft wrap\nhard\nhtml");
}

#[test]
fn inline_html_marks_and_linked_images_survive_markdown_parsing() {
    let parsed = document(
        "Press <kbd>Ctrl</kbd> and <strong>read</strong> <a href=\"../guide.md\"><img src=\"icon.svg\" alt=\"Guide\"></a>.",
        true,
    );
    let BlockNode::Paragraph(paragraph) = &parsed.blocks[0] else {
        panic!("paragraph");
    };
    assert!(
        paragraph
            .children
            .iter()
            .any(|node| node.text == "Ctrl" && node.marks.iter().any(|(_, mark)| mark.code))
    );
    assert!(
        paragraph
            .children
            .iter()
            .any(|node| node.text == "read" && node.marks.iter().any(|(_, mark)| mark.bold))
    );
    assert!(
        paragraph
            .children
            .iter()
            .any(|node| node.image.as_ref().is_some_and(|image| image
                .link
                .as_ref()
                .is_some_and(|link| link.url == "../guide.md")))
    );
}

#[test]
fn html_containers_inherit_alignment_and_keep_child_overrides() {
    let parsed = format::html::parse("<div align=\"center\"><p>Centered<br>line</p><p align=\"right\">Right</p><script>hidden()</script></div>", &mut NodeContext::default()).unwrap();
    fn paragraphs(nodes: &[BlockNode], output: &mut Vec<(String, Option<gpui::TextAlign>)>) {
        for node in nodes {
            match node {
                BlockNode::Root { children, .. } => paragraphs(children, output),
                BlockNode::Paragraph(p) => output.push((p.text(), p.alignment)),
                _ => {}
            }
        }
    }
    let mut output = Vec::new();
    paragraphs(&parsed.blocks, &mut output);
    assert!(
        output
            .iter()
            .any(|(text, align)| text.contains("Centered")
                && *align == Some(gpui::TextAlign::Center))
    );
    assert!(
        output
            .iter()
            .any(|(text, align)| text == "Right" && *align == Some(gpui::TextAlign::Right))
    );
    assert!(!parsed.text().contains("hidden()"));
}

#[test]
fn readme_container_alignment_crosses_markdown_blocks_and_stops_at_the_close() {
    let parsed = document(
        "<div align=\"center\">\n\n<img src=\"assets/app-icon.svg\" width=\"88\" height=\"88\" />\n\n### tty7 · Custom Fork\n\n**A maintained fork.**\n\n<p align=\"right\">Right</p>\n\n</div>\n\n## Outside",
        true,
    );
    let headings: Vec<_> = parsed
        .blocks
        .iter()
        .filter_map(|node| match node {
            BlockNode::Heading { children, .. } => Some((children.text(), children.alignment)),
            _ => None,
        })
        .collect();
    assert_eq!(
        headings,
        [
            ("tty7 · Custom Fork".into(), Some(gpui::TextAlign::Center)),
            ("Outside".into(), None)
        ]
    );
    assert!(parsed.blocks.iter().any(|node| matches!(node, BlockNode::Paragraph(p) if p.text() == "A maintained fork." && p.alignment == Some(gpui::TextAlign::Center))));
}

#[test]
fn heading_ids_are_stable_and_disambiguate_repeated_unicode_titles() {
    let mut parsed = document(
        "# Hello, world!\n\n## 重复标题\n\n## 重复标题\n\n## 重复标题-1",
        true,
    );
    parsed.assign_anchors();
    let names: Vec<_> = parsed
        .blocks
        .iter()
        .filter_map(|node| match node {
            BlockNode::Heading { anchor, .. } => Some(anchor.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        ["hello-world", "重复标题", "重复标题-1", "重复标题-1-1"]
    );
}

#[test]
fn unchanged_code_uses_updated_colors_even_when_theme_name_stays_the_same() {
    let red: gpui::Hsla = gpui::rgb(0xcc3300).into();
    let blue: gpui::Hsla = gpui::rgb(0x3366ee).into();
    let mut before = HighlightTheme::default_light().as_ref().clone();
    before.style.syntax.number = Some(ThemeStyle::from(red));
    let mut after = before.clone();
    after.style.syntax.number = Some(ThemeStyle::from(blue));
    let before = Arc::new(before);
    let after = Arc::new(after);
    let block = CodeBlock::new(
        "{\"number\": 42}".into(),
        Some("json".into()),
        &before,
        None::<super::node::Span>,
    );
    let old = block.styles_for(&before);
    let new = block.styles_for(&after);
    assert!(old.iter().any(|(_, style)| style.color == Some(red)));
    assert!(new.iter().any(|(_, style)| style.color == Some(blue)));
    assert_ne!(old, new);
    assert_eq!(block.code().as_ref(), "{\"number\": 42}");
    let unknown = CodeBlock::new(
        "plain code".into(),
        Some("not-a-language".into()),
        &before,
        None::<super::node::Span>,
    );
    assert!(
        unknown
            .styles_for(&after)
            .iter()
            .all(|(_, style)| style.color.is_none())
    );
}

#[test]
fn local_style_equality_includes_element_refinements_and_hover_colors() {
    let original = TextViewStyle::default();
    let mut changed = original.clone();
    changed.table_header.text.color = Some(gpui::rgb(0x123456).into());
    assert!(original != changed);
    changed = original.clone();
    changed.link_hover_color = Some(gpui::rgb(0x123456).into());
    assert!(original != changed);
    changed = original.clone();
    changed.is_dark = !original.is_dark;
    assert!(original != changed);
}

struct ReadingRoot {
    local: Entity<TextViewState>,
    ordinary: Entity<TextViewState>,
    style: TextViewStyle,
    links: Arc<Mutex<Vec<String>>>,
    images: Arc<Mutex<Vec<String>>>,
}

impl Render for ReadingRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let links = self.links.clone();
        let images = self.images.clone();
        div()
            .w(px(480.))
            .child(
                TextView::new(&self.local)
                    .selectable(true)
                    .style(self.style.clone())
                    .on_link(move |url, _, _| links.lock().unwrap().push(url.to_owned()))
                    .image_source(move |url, _, _| {
                        images.lock().unwrap().push(url.to_string());
                        TextViewImageSource::Loading
                    }),
            )
            .child(TextView::new(&self.ordinary).selectable(true))
    }
}

#[gpui::test]
fn instance_styles_resources_and_selection_remain_local(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| ReadingRoot {
            local: cx.new(|cx| {
                TextViewState::markdown(
                    "[go](../guide.md#next)\n\n# Target\n\n![alt](relative.png)",
                    cx,
                )
            }),
            ordinary: cx.new(|cx| TextViewState::markdown("ordinary", cx)),
            style: TextViewStyle {
                link_color: Some(gpui::rgb(0xaa1122).into()),
                ..Default::default()
            },
            links: Default::default(),
            images: Default::default(),
        });
        crate::Root::new(content, window, cx)
    });
    let view = root.read_with(vcx, |root, _| {
        root.view().clone().downcast::<ReadingRoot>().unwrap()
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let (local, ordinary) =
        view.read_with(vcx, |root, _| (root.local.clone(), root.ordinary.clone()));
    local.update(vcx, |state, cx| state.select_all(cx));
    vcx.update(|window, cx| crate::Theme::change(crate::ThemeMode::Dark, Some(window), cx));
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    local.read_with(vcx, |state, _| {
        assert!(state.text_view_style.link_color.is_some());
        assert!(state.anchor_bounds("target").is_some());
        assert!(state.selected_text().contains("Target"));
    });
    ordinary.read_with(vcx, |state, cx| {
        assert_eq!(state.text_view_style.link_color, None);
        assert_eq!(
            state.text_view_style.highlight_theme,
            cx.theme().highlight_theme
        );
    });
    view.read_with(vcx, |root, _| {
        assert!(
            root.images
                .lock()
                .unwrap()
                .iter()
                .any(|url| url == "relative.png")
        )
    });
    local.update(vcx, |state, cx| state.clear_selection(cx));
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    vcx.simulate_click(gpui::point(px(12.), px(10.)), gpui::Modifiers::default());
    view.read_with(vcx, |root, _| {
        assert_eq!(*root.links.lock().unwrap(), vec!["../guide.md#next"])
    });
    assert_eq!(vcx.opened_url(), None);
}
