use leptos::{create_memo, create_render_effect, SignalWithUntracked};
use leptos::{
    document, ev, html, mount_to_body, on_cleanup, window_event_listener, CollectView, IntoView,
    RwSignal, Signal, SignalGet, SignalSet, SignalWith,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use web_sys::wasm_bindgen::JsCast;

mod human;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Data {
    unit: String,
    datapoints: Vec<Datapoint>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Datapoint {
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

    let handle = window_event_listener(ev::scroll, move |_| data.with(adjust_size));
    on_cleanup(move || handle.remove());

    let data = data.get();

    set_css_variable("--size", &data.datapoints[0].size.to_string());
    adjust_size(&data);

    [
        html::div().class("histogram", true).child(
            data.datapoints
                .into_iter()
                .map(|Datapoint { name, size, .. }| {
                    let scaled_unit = human::round_with_scaled_unit(size, &data.unit);
                    let scaled_power = human::round_with_power(size, &data.unit);
                    html::div()
                        .class("datapoint", true)
                        .attr("size", size.to_string())
                        .child(format!("{name} — {scaled_unit} ({scaled_power})"))
                        .child(html::div().class("datapoint-bar", true).style(
                            "transform",
                            format!("scaleX(calc(min({size}/var(--size), 1)))"),
                        ))
                })
                .collect_view(),
        ),
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

fn adjust_size(Data { datapoints, .. }: &Data) {
    let elements = document().query_selector_all("[size]").unwrap();

    for i in 0.. {
        let Some(element) = elements.item(i) else {
            return;
        };
        let element: web_sys::HtmlElement = element.dyn_into().unwrap();
        let rect = element.get_bounding_client_rect();
        let i: usize = i.try_into().unwrap();

        let height = rect.height();
        let top = rect.top();

        if -height < top && top < 0. {
            let visible_fraction = (height + top) / height;
            let size = datapoints[i].size;
            let size_next = datapoints.get(i + 1).map(|d| d.size).unwrap_or(0.);
            let global = size_next + (size - size_next) * visible_fraction;
            set_css_variable("--size", &global.to_string());
            return;
        } else if i == 0 && top > 0. {
            set_css_variable("--size", &datapoints[0].size.to_string());
        }
    }
}

fn set_css_variable(name: &str, value: &str) {
    let element: web_sys::HtmlElement = document().document_element().unwrap().dyn_into().unwrap();
    element.style().set_property(name, value).unwrap();
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
