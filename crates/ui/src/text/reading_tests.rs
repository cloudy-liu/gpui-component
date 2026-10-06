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
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestAppContext, Window, div, px,
};

fn document(source: &str, alerts: bool) -> super::document::ParsedDocument {
    let mut context = NodeContext::default();
    if alerts {
        context.markdown_extensions = Arc::new(MarkdownExtensions::default().github_alerts());
    }
    format::markdown::parse(source, &mut context, &HighlightTheme::default_light()).unwrap()
}

#[test]
#[cfg(feature = "tree-sitter-diff")]
fn diff_code_blocks_use_text_span_backgrounds_and_keep_legacy_fallbacks() {
    let mut theme = HighlightTheme::default_light().as_ref().clone();
    theme.style.syntax = serde_json::from_value(serde_json::json!({
        "diff.added": {"color":"#116329", "background_color":"#dafbe1"},
        "diff.deleted": {"color":"#82071e", "background_color":"#ffebe9"},
        "diff.hunk": {"color":"#8250df", "font_weight":700}
    }))
    .unwrap();
    let parsed = document("```diff\n@@ -1 +1 @@\n-old\n+new\n```", true);
    let BlockNode::CodeBlock(block) = &parsed.blocks[0] else {
        panic!("code block")
    };
    let styles = block.styles_for(&Arc::new(theme));
    for (text, background) in [("+new", 0xdafbe1), ("-old", 0xffebe9)] {
        let start = block.code().find(text).unwrap();
        assert!(
            styles.iter().any(|(range, style)| range.contains(&start)
                && style.background_color == Some(gpui::rgb(background).into())),
            "{text}"
        );
    }
    let legacy = HighlightTheme::default_light();
    let styles = block.styles_for(&legacy);
    let start = block.code().find("+new").unwrap();
    assert!(styles.iter().any(|(range, style)| range.contains(&start)
        && Some(*style) == legacy.style.syntax.style("string")));
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

struct PercentageImageRoot {
    text: Entity<TextViewState>,
    image: Arc<gpui::RenderImage>,
    width: f32,
}

impl Render for PercentageImageRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.clone();
        div().w(px(self.width)).child(
            TextView::new(&self.text)
                .image_source(move |_, _, _| TextViewImageSource::Ready(image.clone().into())),
        )
    }
}

#[gpui::test]
fn percentage_readme_image_reserves_its_scaled_height(cx: &mut TestAppContext) {
    cx.update(crate::init);
    for percentage in [100., 50.] {
        let source = format!(
            "<div align=\"center\"><img src=\"hero.webp\" width=\"{percentage}%\" /></div>\n\n<br/>\n\n## After"
        );
        let (root, vcx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| PercentageImageRoot {
                text: cx.new(|cx| TextViewState::markdown(&source, cx)),
                image: Arc::new(gpui::RenderImage::new(smallvec::smallvec![
                    image::Frame::new(image::RgbaImage::new(1000, 500))
                ])),
                width: 400.,
            });
            crate::Root::new(view, window, cx)
        });
        let view = root.read_with(vcx, |root, _| {
            root.view()
                .clone()
                .downcast::<PercentageImageRoot>()
                .unwrap()
        });
        for width in [400., 600.] {
            view.update(vcx, |view, cx| {
                view.width = width;
                cx.notify();
            });
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let after = view.read_with(vcx, |view, cx| {
                view.text.read(cx).anchor_bounds("after").unwrap().top()
            });
            let scaled_height = width * percentage / 100. / 2.;
            assert!(
                (scaled_height..scaled_height + 100.).contains(&f32::from(after)),
                "incorrect image height at width {width}, {percentage}%: heading starts at {after:?}"
            );
        }
    }
}

#[gpui::test]
fn heading_code_metrics_reflow_without_losing_selection(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let code = super::InlineCodeStyle {
        radius: px(6.),
        padding_x: px(120.),
        padding_y: px(20.),
        font_size: px(24.),
    };
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| ReadingRoot {
            local: cx.new(|cx| {
                TextViewState::markdown("## before `code words code words code words` after", cx)
            }),
            ordinary: cx.new(|cx| TextViewState::markdown("ordinary", cx)),
            style: TextViewStyle {
                inline_code: Some(code.clone()),
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
    let text = view.read_with(vcx, |root, _| root.local.clone());
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    text.update(vcx, |text, cx| text.select_all(cx));
    let selected = text.read_with(vcx, |text, _| text.selected_text());
    let before = text.read_with(vcx, |text, _| {
        text.anchor_bounds("before-code-words-code-words-code-words-after")
            .unwrap()
    });
    // A heading anchor starts at the heading, not after its last text line.
    assert_eq!(before.top(), px(0.));
    assert_eq!(
        text.read_with(vcx, |text, _| text.block_bounds(0).unwrap().top()),
        px(0.)
    );
    view.update(vcx, |view, cx| {
        view.style.heading_inline_code[1] = Some(super::InlineCodeStyle {
            padding_x: px(4.8),
            padding_y: px(0.),
            ..code
        });
        cx.notify();
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let after = text.read_with(vcx, |text, _| {
        text.anchor_bounds("before-code-words-code-words-code-words-after")
            .unwrap()
    });
    assert!(after.size.height < before.size.height);
    assert_eq!(after.top(), px(0.));
    assert_eq!(
        text.read_with(vcx, |text, _| text.selected_text()),
        selected
    );
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
fn fractional_code_chip_keeps_every_character_on_the_selected_line(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReadingRoot {
            local: cx.new(|cx| TextViewState::markdown("Leading text `code` trailing text", cx)),
            ordinary: cx.new(|cx| TextViewState::markdown("", cx)),
            style: TextViewStyle {
                inline_code: Some(super::InlineCodeStyle {
                    radius: px(6.),
                    padding_x: px(5.44),
                    padding_y: px(2.72),
                    font_size: px(13.6),
                }),
                ..Default::default()
            },
            links: Default::default(),
            images: Default::default(),
        });
        crate::Root::new(view, window, cx)
    });
    let text = root.read_with(vcx, |root, cx| {
        root.view()
            .clone()
            .downcast::<ReadingRoot>()
            .unwrap()
            .read(cx)
            .local
            .clone()
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    vcx.simulate_mouse_down(
        gpui::point(px(0.), px(10.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_move(
        gpui::point(px(479.), px(10.)),
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        gpui::point(px(479.), px(10.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert_eq!(
        text.read_with(vcx, |text, _| text.selected_text()).trim(),
        "Leading text code trailing text"
    );
}

#[gpui::test]
fn heading_layout_keeps_the_inherited_font_when_measurement_is_deferred(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReadingRoot {
            local: cx.new(|cx| TextViewState::markdown("## Heading with `inline code`", cx)),
            ordinary: cx.new(|cx| TextViewState::markdown("", cx)),
            style: TextViewStyle {
                headings: std::array::from_fn(|_| {
                    let mut heading = div()
                        .text_size(px(24.))
                        .line_height(gpui::relative(1.25))
                        .pb_0();
                    heading.style().clone()
                }),
                inline_code: Some(super::InlineCodeStyle {
                    radius: px(6.),
                    padding_x: px(4.8),
                    padding_y: px(0.),
                    font_size: px(24.),
                }),
                ..Default::default()
            },
            links: Default::default(),
            images: Default::default(),
        });
        crate::Root::new(view, window, cx)
    });
    let text = root.read_with(vcx, |root, cx| {
        root.view()
            .clone()
            .downcast::<ReadingRoot>()
            .unwrap()
            .read(cx)
            .local
            .clone()
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let bounds = text.read_with(vcx, |text, _| {
        text.anchor_bounds("heading-with-inline-code").unwrap()
    });
    assert_eq!(
        bounds.size.height,
        px(30.),
        "heading layout must use 24px / 1.25 metrics, not the ambient body font"
    );
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

#[gpui::test]
fn heading_permalink_receives_clicks_outside_the_heading_text(cx: &mut TestAppContext) {
    struct PermalinkRoot {
        text: Entity<TextViewState>,
        links: Arc<Mutex<Vec<String>>>,
    }
    impl Render for PermalinkRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let links = self.links.clone();
            div().p(px(32.)).child(
                TextView::new(&self.text)
                    .selectable(true)
                    .style(TextViewStyle {
                        heading_permalink_icon: Some("icons/link.svg".into()),
                        ..Default::default()
                    })
                    .on_link(move |url, _, _| links.lock().unwrap().push(url.to_owned())),
            )
        }
    }
    cx.update(crate::init);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| PermalinkRoot {
            text: cx.new(|cx| TextViewState::markdown("## Target", cx)),
            links: Default::default(),
        });
        crate::Root::new(view, window, cx)
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    vcx.simulate_mouse_move(
        gpui::point(px(50.), px(42.)),
        None,
        gpui::Modifiers::default(),
    );
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    vcx.simulate_click(gpui::point(px(16.), px(42.)), gpui::Modifiers::default());
    root.read_with(vcx, |root, cx| {
        let view = root.view().clone().downcast::<PermalinkRoot>().unwrap();
        assert_eq!(*view.read(cx).links.lock().unwrap(), vec!["#target"]);
    });
}

#[gpui::test]
fn code_and_keyboard_chips_preserve_copy_and_selection_when_styles_change(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let content = cx.new(|cx| ReadingRoot {
            local: cx.new(|cx| {
                TextViewState::markdown(
                    "Press <kbd>Ctrl</kbd> and `code` beside ordinary text.",
                    cx,
                )
            }),
            ordinary: cx.new(|cx| TextViewState::markdown("ordinary", cx)),
            style: TextViewStyle::default(),
            links: Default::default(),
            images: Default::default(),
        });
        crate::Root::new(content, window, cx)
    });
    let view = root.read_with(vcx, |root, _| {
        root.view().clone().downcast::<ReadingRoot>().unwrap()
    });
    let text = view.read_with(vcx, |root, _| root.local.clone());
    let chips = TextViewStyle {
        inline_code_background: Some(gpui::rgb(0xf6f8fa).into()),
        inline_code: Some(super::InlineCodeStyle {
            radius: px(6.),
            padding_x: px(5.44),
            padding_y: px(2.72),
            font_size: px(13.6),
        }),
        keyboard: Some(super::KeyboardStyle {
            background: gpui::rgb(0xf6f8fa).into(),
            border: gpui::rgb(0xd1d9e0).into(),
            shadow: gpui::rgb(0xd1d9e0).into(),
            radius: px(6.),
            padding: px(4.),
            font_size: px(11.),
            line_height: px(10.),
        }),
        ..Default::default()
    };
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    text.update(vcx, |text, cx| text.select_all(cx));
    let selected = text.read_with(vcx, |text, _| text.selected_text());
    assert_eq!(selected.trim(), "Press Ctrl and code beside ordinary text.");
    for style in [chips.clone(), TextViewStyle::default(), chips] {
        view.update(vcx, |view, cx| {
            view.style = style;
            cx.notify();
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert_eq!(
            text.read_with(vcx, |text, _| text.selected_text()),
            selected
        );
    }
    text.update(vcx, |text, cx| text.clear_selection(cx));
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    // Select across body, kbd and code fragments through the public pointer path.
    vcx.simulate_mouse_down(
        gpui::point(px(1.), px(9.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_move(
        gpui::point(px(200.), px(9.)),
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        gpui::point(px(200.), px(9.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let partial = text.read_with(vcx, |text, _| text.selected_text());
    assert!(
        partial.contains("Ctrl") && partial.contains("code"),
        "{partial:?}"
    );
    view.update(vcx, |view, cx| {
        view.style = TextViewStyle::default();
        cx.notify();
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert_eq!(text.read_with(vcx, |text, _| text.selected_text()), partial);
}

#[gpui::test]
fn inline_code_wraps_at_narrow_width_and_pointer_copy_keeps_all_source_bytes(
    cx: &mut TestAppContext,
) {
    struct NarrowReading {
        text: Entity<TextViewState>,
    }
    impl Render for NarrowReading {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("narrow-code")
                .w(px(180.))
                .text_size(px(16.))
                .child(
                    TextView::new(&self.text)
                        .selectable(true)
                        .style(TextViewStyle {
                            paragraph_gap: gpui::rems(0.),
                            inline_code: Some(super::InlineCodeStyle {
                                font_size: px(13.6),
                                radius: px(6.),
                                padding_x: px(5.44),
                                padding_y: px(2.72),
                            }),
                            keyboard: Some(super::KeyboardStyle {
                                background: gpui::rgb(0xf6f8fa).into(),
                                border: gpui::rgb(0xd1d9e0).into(),
                                shadow: gpui::rgb(0xd1d9e0).into(),
                                font_size: px(11.),
                                line_height: px(10.),
                                radius: px(6.),
                                padding: px(4.),
                            }),
                            ..Default::default()
                        }),
                )
        }
    }
    cx.update(crate::init);
    let phrase = "ab cd ef gh ij kl mn op qr st uv wx yz";
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| NarrowReading {
            text: cx.new(|cx| {
                TextViewState::markdown(&format!("`{phrase}`\n\n`xx`\n\n<kbd>{phrase}</kbd>"), cx)
            }),
        });
        crate::Root::new(view, window, cx)
    });
    let text = root.read_with(vcx, |root, cx| {
        root.view()
            .clone()
            .downcast::<NarrowReading>()
            .unwrap()
            .read(cx)
            .text
            .clone()
    });
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let bounds = text.read_with(vcx, |text, _| text.block_bounds(0).unwrap());
    let single_line = text.read_with(vcx, |text, _| text.block_bounds(1).unwrap().size.height);
    assert!(
        bounds.size.height > px(50.),
        "long code must wrap: {bounds:?}"
    );
    assert!(
        (bounds.size.height - single_line * 2.).abs() < px(1.),
        "code should fit two lines with padding once per line: {bounds:?}, single line {single_line:?}"
    );
    let keyboard_height = text.read_with(vcx, |text, _| text.block_bounds(2).unwrap().size.height);
    assert!(
        keyboard_height <= single_line + px(1.),
        "keyboard labels stay atomic: {keyboard_height:?} vs single code line {single_line:?}"
    );
    // This fixture places its only paragraph at the window origin. The block
    // callback reports its helper canvas position, so use its measured size.
    let bounds = gpui::Bounds::new(gpui::point(px(0.), px(0.)), bounds.size);
    vcx.simulate_mouse_down(
        bounds.origin + gpui::point(px(6.), px(6.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    let end = gpui::point(bounds.right() - px(1.), bounds.bottom() - px(1.));
    vcx.simulate_mouse_move(
        end,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert_eq!(
        text.read_with(vcx, |text, _| text.selected_text()).trim(),
        phrase
    );
}
