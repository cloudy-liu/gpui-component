use std::sync::Arc;

use gpui::{
    AnyElement, App, DefiniteLength, ImageSource, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, SharedUri, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window, div, img, prelude::FluentBuilder as _, relative,
};

use crate::{WindowExt as _, tooltip::Tooltip};

use super::node::LinkMark;

/// An application can resolve document images without exposing its filesystem
/// or transport to the text renderer. Loading and failures retain the alt text.
#[derive(Clone)]
pub enum TextViewImageSource {
    Ready(ImageSource),
    Loading,
    Failed(SharedString),
}

pub(super) type LinkHandler = dyn Fn(&str, &mut Window, &mut App) + Send + Sync;
pub(super) type ImageResolver =
    dyn Fn(&SharedUri, &mut Window, &mut App) -> TextViewImageSource + Send + Sync;
pub(super) type ImageActions =
    dyn Fn(&SharedUri, &mut Window, &mut App) -> Option<AnyElement> + Send + Sync;

#[derive(Clone, Default)]
pub(super) struct TextViewInteractions {
    pub on_link: Option<Arc<LinkHandler>>,
    pub image_source: Option<Arc<ImageResolver>>,
    pub image_actions: Option<Arc<ImageActions>>,
}

impl TextViewInteractions {
    pub fn open_link(&self, url: &str, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_link {
            handler(url, window, cx);
        } else {
            cx.open_url(url);
        }
    }

    pub fn image_source(
        &self,
        url: &SharedUri,
        window: &mut Window,
        cx: &mut App,
    ) -> TextViewImageSource {
        self.image_source.as_ref().map_or_else(
            || TextViewImageSource::Ready(url.clone().into()),
            |resolve| resolve(url, window, cx),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn image(
        &self,
        ix: usize,
        url: &SharedUri,
        link: &Option<LinkMark>,
        title: &str,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let alt: SharedString = if title.is_empty() {
            "Image".into()
        } else {
            title.to_owned().into()
        };
        let content = match self.image_source(url, window, cx) {
            TextViewImageSource::Ready(source) => {
                let failed = alt.clone();
                let loading = alt.clone();
                img(source)
                    .id("playback")
                    .object_fit(ObjectFit::Contain)
                    .max_w(relative(1.))
                    .when_some(width, |this, width| this.w(width))
                    .when_some(height, |this, height| this.h(height))
                    .with_loading(move || {
                        div()
                            .whitespace_nowrap()
                            .child(format!("{} …", loading))
                            .into_any_element()
                    })
                    .with_fallback(move || {
                        div()
                            .whitespace_nowrap()
                            .child(format!("[{}]", failed))
                            .into_any_element()
                    })
                    .into_any_element()
            }
            TextViewImageSource::Loading => div()
                .whitespace_nowrap()
                .child(format!("{} …", alt))
                .into_any_element(),
            TextViewImageSource::Failed(error) => div()
                .id("image-error")
                .whitespace_nowrap()
                .child(format!("[{}]", alt))
                .tooltip(move |window, cx| Tooltip::new(error.clone()).build(window, cx))
                .into_any_element(),
        };
        let interactions = self.clone();
        div()
            .id(("image", ix))
            .relative()
            .max_w(relative(1.))
            .child(content)
            .when_some(
                self.image_actions
                    .as_ref()
                    .and_then(|actions| actions(url, window, cx)),
                |this, actions| {
                    this.child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .child(actions),
                    )
                },
            )
            .when_some(link.clone(), |this, link| {
                this.cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(alt.clone()).build(window, cx))
                    .on_click(move |_, window, cx| {
                        window.end_text_selection(cx);
                        cx.stop_propagation();
                        interactions.open_link(&link.url, window, cx);
                    })
            })
            .into_any_element()
    }
}
