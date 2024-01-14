use leptos::html::ToHtmlElement;
use leptos::{create_memo, create_render_effect, SignalWithUntracked};
use leptos::{
    document, ev, html, mount_to_body, on_cleanup, window_event_listener, CollectView, IntoView,
    RwSignal, Signal, SignalGet, SignalSet, SignalWith,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use visibility::{ViewportSize, Visibility};
use web_sys::wasm_bindgen::JsCast;
use web_sys::{HtmlDivElement, Node};

pub mod human;
pub mod visibility;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Data {
    unit: String,
    datapoints: Vec<Datapoint>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Datapoint {
    name: String,
    size: f64,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    standard_uncertainty: Option<f64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Histogram,
    RawData,
}

pub fn main() {
    console_log::init().unwrap();

    mount_to_body(app);
}

fn app() -> impl IntoView {
    let tab = RwSignal::new(Tab::Histogram);
    let data = RwSignal::new(initial_data());

    (
        html::div()
            .class("navigation", true)
            .child(
                html::button()
                    .class("tab-button", true)
                    .child("Histogram")
                    .on(ev::click, move |_| tab.set(Tab::Histogram)),
            )
            .child(
                html::button()
                    .class("tab-button", true)
                    .child("Raw Data")
                    .on(ev::click, move |_| tab.set(Tab::RawData)),
            ),
        move || match tab.get() {
            Tab::Histogram => histogram(data.into()).into_view(),
            Tab::RawData => raw_data(data).into_view(),
        },
    )
}

fn histogram(data: Signal<Data>) -> impl IntoView {
    let data = create_memo(move |_| {
        let mut data = data.get();
        data.datapoints.extend(human::prefix_datapoints());
        data.sort();
        data
    });

    let elements: Signal<Vec<(HtmlDivElement, HtmlDivElement)>> = create_memo(move |_| {
        let data = data.get();
        data.datapoints
            .into_iter()
            .map(|Datapoint { name, size, .. }| {
                let scaled_unit = human::round_with_scaled_unit(size, &data.unit);
                let scaled_power = human::round_with_power(size, &data.unit);

                let bar = html::div().class("datapoint-bar", true);
                let datapoint = html::div()
                    .class("datapoint", true)
                    .child(format!("{name} — {scaled_unit} ({scaled_power})"))
                    .child(bar.clone());

                (datapoint.deref().clone(), bar.deref().clone())
            })
            .collect()
    })
    .into();

    {
        let frame = Rc::new(RefCell::new(None));
        let handle = window_event_listener(ev::scroll, move |_| {
            let inner = frame.clone();
            let new = frame.take().unwrap_or_else(move || {
                gloo_render::request_animation_frame(move |_| {
                    data.with(move |data| {
                        elements.with(|elements| {
                            adjust_size(data, elements);
                        });
                    });
                    drop(inner.take());
                })
            });
            frame.replace(Some(new));
        });
        on_cleanup(move || handle.remove());
    }

    [
        html::div()
            .class("histogram", true)
            .child(move || {
                elements
                    .get()
                    .into_iter()
                    .map(|(datapoint, _)| datapoint.to_leptos_element())
                    .collect_view()
            })
            .on_mount(move |_| {
                data.with(move |data| {
                    elements.with(|elements| {
                        adjust_size(data, elements);
                    });
                });
            }),
        html::div().style("height", "100vh"),
    ]
}

fn raw_data(data: RwSignal<Data>) -> impl IntoView {
    let raw = RwSignal::new(data.with_untracked(|data| data.to_json()));

    let parsed = create_memo(move |_| raw.with(|raw| Data::from_json(raw)));

    let first = RefCell::new(true);
    create_render_effect(move |_| {
        parsed.track();

        if *first.borrow() {
            *first.borrow_mut() = false;
            return;
        }

        if let Ok(mut new) = parsed.get() {
            new.sort();
            data.set(new);
        }
    });

    html::div()
        .class("raw-data", true)
        .child(move || {
            if parsed.with(|parsed| parsed.is_ok()) {
                "Valid Json."
            } else {
                "Invalid Json"
            }
        })
        .child(
            html::pre().class("raw-data-container", true).child(
                html::textarea()
                    .class("raw-data-input", true)
                    .on(ev::input, move |e| raw.set(leptos::event_target_value(&e)))
                    .prop("value", raw),
            ),
        )
}

fn adjust_size(data: &Data, elements: &[(HtmlDivElement, HtmlDivElement)]) {
    let scale = get_scale(data, elements);
    let overflow = "scaleX(1)";
    let underflow = "scaleX(0)";

    for (datapoint, (_, bar)) in data.datapoints.iter().zip(elements.to_vec()) {
        if datapoint.size > scale {
            bar.style().set_property("transform", overflow).unwrap();
        } else if datapoint.size < (scale / 10_000.) {
            bar.style().set_property("transform", underflow).unwrap()
        } else {
            bar.style()
                .set_property(
                    "transform",
                    &format!("scaleX({:.3})", datapoint.size / scale),
                )
                .unwrap();
        };
    }
}
fn get_scale(Data { datapoints, .. }: &Data, elements: &[(HtmlDivElement, HtmlDivElement)]) -> f64 {
    let (reference, visibility) = reference(elements);
    let size = match datapoints.get(reference) {
        Some(reference) => reference.size,
        None => {
            assert_eq!(0, datapoints.len());
            1e30
        }
    };
    let size_next = match datapoints.get(reference + 1) {
        Some(reference) => reference.size,
        None => 0.,
    };
    scale_1(visibility, size, size_next)
}
/// Returns the reference element, and how much we should scale into that element.
fn reference(elements: &[(HtmlDivElement, HtmlDivElement)]) -> (usize, f64) {
    let view = ViewportSize::from_global();

    for (i, (datapoint, _)) in elements.iter().enumerate() {
        let rect = datapoint.get_bounding_client_rect();
        match Visibility::vertical_from_rect(&rect, &view) {
            Visibility::Before => {}
            Visibility::PeekingBefore(fraction) => return (i, fraction),
            Visibility::Inside => return (i, 1.),
            Visibility::PeekingAfter(fraction) => return (i, fraction),
            Visibility::After => return (i, 0.),
            Visibility::Straddling(fraction) => return (i, fraction),
        }
    }

    (elements.len().saturating_sub(1), 0.)
}

/// Scales the size linearly such that:
/// - At the beginning of the current bounding box it returns `size`.
/// - At the end of the current bounding box it returns `size_next`.
/// - Midway through the current bounding box it returns `(size + size_next)/2`.
///
/// This means that the returned value changes linearly in `visible_fraction`,
/// and the next histogram bar will appear to grow more slowly at the beginning.
/// This is because the linear descreases in the scaled size will be a larger
/// proportion of it as we continue.
///
/// This scale gives more of a feeling for what big jumps are actually like.
fn scale_1(visible_fraction: f64, size: f64, size_next: f64) -> f64 {
    interpolate(size, size_next, visible_fraction) // size -> size_next
}
/// Scales the the size such that:
/// - At the beginning of the current bounding box it returns `size`.
/// - At the end of the current bounding box it returns `size_next`.
/// - Midway through the current bounding box it returns the size needed for the
///   next bar to be midway between where it was at the beginning of the current
///   bounding box and full width (which is also where it will be at the end).
///
/// This means that the next histogram bar will be growing linearly.
///
/// This scale 'feels' better, UI wise, but big jumps will be 'harder to see'.
#[allow(dead_code)]
fn scale_2(visible_fraction: f64, size: f64, size_next: f64) -> f64 {
    // Initial fractional size of the next one when at the top of current bounding box.
    let ratio = size_next / size;

    let scaling_factor = interpolate(ratio, 1., visible_fraction); // ratio -> 1.

    size_next / scaling_factor
}
fn interpolate(from: f64, to: f64, progress: f64) -> f64 {
    to + (from - to) * progress
}

#[allow(dead_code)]
fn set_css_variable(name: &str, value: &str) {
    let element: web_sys::HtmlElement = document().document_element().unwrap().dyn_into().unwrap();
    element.style().set_property(name, value).unwrap();
}

#[allow(dead_code)]
fn select(selector: &str) -> impl Iterator<Item = Node> {
    let elements = document().query_selector_all(selector).unwrap();
    // Returned [NodeList] is static.
    // https://developer.mozilla.org/en-US/docs/Web/API/Document/querySelectorAll#return_value
    (0..elements.length()).map(move |i| elements.item(i).expect("static (non-live) `NodeList`"))
}

impl Data {
    fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
    fn from_json(raw: &str) -> Result<Self, ()> {
        serde_json::from_str::<Data>(raw).map_err(drop)
    }
    fn sort(&mut self) {
        self.datapoints
            .sort_by(|a, b| f64::total_cmp(&a.size, &b.size).reverse());
    }
}

fn initial_data() -> Data {
    const INTIAL_DATA: &str = include_str!("./lengths.json");
    Data::from_json(INTIAL_DATA).unwrap()
}
