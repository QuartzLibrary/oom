use leptos::{create_memo, create_render_effect, SignalWithUntracked};
use leptos::{
    document, ev, html, mount_to_body, on_cleanup, window_event_listener, CollectView, IntoView,
    RwSignal, Signal, SignalGet, SignalSet, SignalWith,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    cmp::{self, Ordering},
    ops::RangeInclusive,
};
use web_sys::wasm_bindgen::JsCast;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Data {
    unit: String,
    datapoints: Vec<Datapoint>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Datapoint {
    name: String,
    size: f64,
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
    let handle = window_event_listener(ev::scroll, move |_| data.with(adjust_size));
    on_cleanup(move || handle.remove());

    let data = data.get();

    set_css_variable("--size", &data.datapoints[0].size.to_string());
    adjust_size(&data);

    [
        html::div().class("histogram", true).child(
            data.datapoints
                .into_iter()
                .map(|Datapoint { name, size }| {
                    let human_readable_size = human_readable(size, &data.unit);
                    html::div()
                        .class("datapoint", true)
                        .attr("size", size.to_string())
                        .child(format!("{name} {human_readable_size}"))
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
    let raw =
        RwSignal::new(data.with_untracked(|data| serde_json::to_string_pretty(data).unwrap()));

    let parsed =
        create_memo(move |_| raw.with(|raw| serde_json::from_str::<Data>(raw).map_err(drop)));

    let first = RefCell::new(true);
    create_render_effect(move |_| {
        parsed.track();

        if *first.borrow() {
            *first.borrow_mut() = false;
            return;
        }

        if let Ok(mut new) = parsed.get() {
            new.datapoints
                .sort_by(|a, b| f64::total_cmp(&a.size, &b.size).reverse());
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

fn human_readable(number: f64, unit: &str) -> String {
    // 			Prefix 			Base 10 	Decimal 							Adoption
    // Name 		Symbol
    // quetta 		Q 			10^+30 		1000000000000000000000000000000 	2022
    // ronna 		R 			10^+27 		1000000000000000000000000000 		2022
    // yotta 		Y 			10^+24 		1000000000000000000000000 			1991
    // zetta 		Z 			10^+21 		1000000000000000000000 				1991
    // exa 			E 			10^+18 		1000000000000000000 				1975
    // peta 		P 			10^+15 		1000000000000000 					1975
    // tera 		T 			10^+12 		1000000000000 						1960
    // giga 		G 			10^+9 		1000000000 							1960
    // mega 		M 			10^+6 		1000000 							1873
    // kilo 		k 			10^+3 		1000 								1795
    // — 			— 			10^+0 		1 									—
    // milli 		m 			10^−3 		0.001 								1795
    // micro 		μ 			10^−6 		0.000001 							1873
    // nano 		n 			10^−9 		0.000000001 						1960
    // pico 		p 			10^−12 		0.000000000001 						1960
    // femto 		f 			10^−15 		0.000000000000001 					1964
    // atto 		a 			10^−18 		0.000000000000000001 				1964
    // zepto 		z 			10^−21 		0.000000000000000000001 			1991
    // yocto 		y 			10^−24 		0.000000000000000000000001 			1991
    // ronto 		r 			10^−27 		0.000000000000000000000000001 		2022
    // quecto 		q 			10^−30 		0.000000000000000000000000000001 	2022

    const LARGE_PREFIXES: [&str; 11] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];
    const SMALL_PREFIXES: [&str; 11] = ["", "m", "μ", "n", "p", "f", "a", "z", "y", "r", "q"];
    assert_eq!(LARGE_PREFIXES.len(), SMALL_PREFIXES.len());

    let max_prefix: f64 = (LARGE_PREFIXES.len() - 1) as f64;

    let order_of_magnitude = number.abs().log10().floor();
    let bounded_index = clamp(
        (order_of_magnitude / 3.).floor(),
        (-max_prefix)..=max_prefix,
        f64::total_cmp,
    ) as isize;
    let scaled_number = number / f64::powi(10., bounded_index as i32 * 3);

    let prefix = match Ord::cmp(&bounded_index, &0) {
        Ordering::Less => SMALL_PREFIXES[bounded_index.unsigned_abs()],
        Ordering::Equal => "",
        Ordering::Greater => LARGE_PREFIXES[bounded_index as usize],
    };

    let scaled_number = format!("{scaled_number:.3}");
    let scaled_number = scaled_number.trim_end_matches('0').trim_end_matches('.');

    format!("{scaled_number} {prefix}{unit}")
}

fn clamp<T: Clone>(v: T, range: RangeInclusive<T>, mut f: impl FnMut(&T, &T) -> Ordering) -> T {
    cmp::min_by(
        cmp::max_by(v, range.start().clone(), &mut f),
        range.end().clone(),
        f,
    )
}

fn initial_data() -> Data {
    Data {
        unit: "m".to_owned(),
        datapoints: vec![
            Datapoint {
                name: "Neptune".to_owned(),
                size: 4498396441000.,
            },
            Datapoint {
                name: "Uranus".to_owned(),
                size: 2870658186000.,
            },
            Datapoint {
                name: "Saturn".to_owned(),
                size: 1426666422000.,
            },
            Datapoint {
                name: "Jupiter".to_owned(),
                size: 778340821000.,
            },
            Datapoint {
                name: "Mars".to_owned(),
                size: 227943824000.,
            },
            Datapoint {
                name: "Earth".to_owned(),
                size: 149598262000.,
            },
            Datapoint {
                name: "Venus".to_owned(),
                size: 108209475000.,
            },
            Datapoint {
                name: "Mercury".to_owned(),
                size: 57909227000.,
            },
        ],
    }
}
