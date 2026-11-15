use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use palette::{FromColor, LinSrgb, Mix, Oklch, Srgb};

use crate::history::BucketCount;

const DAYS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];
const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

#[derive(Clone, Copy)]
struct Cell {
    date: NaiveDate,
    x: u32,
    y: u32,
}

#[derive(Clone, Debug)]
pub struct SvgOptions {
    pub cell_size: u32,
    pub cell_gap: u32,
    pub show_month_labels: bool,
    pub show_day_labels: bool,
    pub base_color: String,
    pub text_color: String,
    pub cell_background: String,
}

pub fn generate_contribution_graph(data: &[BucketCount], options: &SvgOptions) -> String {
    let (first, last) = range(data);
    let start = first - Duration::days(i64::from(first.weekday().num_days_from_sunday()));
    let counts: HashMap<_, _> = data
        .iter()
        .filter_map(|item| parse_date(&item.date).map(|date| (date, item.count)))
        .collect();
    let intensities = intensity_map(&counts);
    let cell_step = options.cell_size + options.cell_gap;
    let weeks = ((last - start).num_days() as u32 / 7) + 1;
    let left = if options.show_day_labels { 30 } else { 10 };
    let top = if options.show_month_labels { 20 } else { 10 };
    let width = left + weeks * cell_step + 10;
    let height = top + 7 * cell_step + 35;

    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="Command history contribution graph">"#,
    );
    if options.show_day_labels {
        for (index, day) in DAYS.iter().enumerate() {
            let y = top + index as u32 * cell_step + options.cell_size / 2;
            let _ = write!(
                svg,
                r#"<text x="12" y="{y}" fill="{}" font-size="10" font-family="monospace" dominant-baseline="middle">{day}</text>"#,
                safe_color(&options.text_color)
            );
        }
    }

    let mut date = start;
    let mut previous_month = None;
    let mut cells = Vec::new();
    while date <= last {
        let day = date.weekday().num_days_from_sunday();
        let week = (date - start).num_days() as u32 / 7;
        let x = left + week * cell_step;
        let y = top + day * cell_step;
        cells.push(Cell { date, x, y });
        let count = counts.get(&date).copied().unwrap_or(0);
        let fill = if count == 0 {
            safe_color(&options.cell_background).to_owned()
        } else {
            intensity_color(
                &options.base_color,
                &options.cell_background,
                intensities.get(&date).copied().unwrap_or(1),
            )
        };
        let _ = write!(
            svg,
            r#"<rect x="{x}" y="{y}" width="{}" height="{}" fill="{fill}" rx="2"><title>{date}: {count}</title></rect>"#,
            options.cell_size, options.cell_size
        );

        if options.show_month_labels && previous_month != Some(date.month()) {
            if date.day() <= 7 || previous_month.is_some() {
                let label = MONTHS[date.month0() as usize];
                let _ = write!(
                    svg,
                    r#"<text x="{x}" y="12" fill="{}" font-size="10" font-family="monospace">{label}</text>"#,
                    safe_color(&options.text_color)
                );
            }
            previous_month = Some(date.month());
        }
        date += Duration::days(1);
    }

    render_month_separators(&mut svg, &cells, options, top);

    let total: i64 = data.iter().map(|item| item.count).sum();
    let days = (last - first).num_days() + 1;
    let footer_y = height - 15;
    let legend_start = left + 35;
    let _ = write!(
        svg,
        r#"<text x="{left}" y="{footer_y}" fill="{}" font-size="11" font-family="monospace" dominant-baseline="middle">Less</text>"#,
        safe_color(&options.text_color)
    );
    for (index, level) in [0, 1, 3, 5, 6, 7, 8, 9].into_iter().enumerate() {
        let x = legend_start + index as u32 * 13;
        let fill = if level == 0 {
            safe_color(&options.cell_background).to_owned()
        } else {
            intensity_color(&options.base_color, &options.cell_background, level)
        };
        let _ = write!(
            svg,
            r#"<rect x="{x}" y="{}" width="10" height="10" fill="{fill}" rx="2"/>"#,
            footer_y - 5
        );
    }
    let more_x = legend_start + 8 * 13 + 5;
    let _ = write!(
        svg,
        r#"<text x="{more_x}" y="{footer_y}" fill="{}" font-size="11" font-family="monospace" dominant-baseline="middle">More</text><text x="{}" y="{footer_y}" fill="{}" font-size="11" font-family="monospace" dominant-baseline="middle" text-anchor="end">{total} commands over {days} days</text>"#,
        safe_color(&options.text_color),
        width - 10,
        safe_color(&options.text_color),
    );
    svg.push_str("</svg>");
    svg
}

fn range(data: &[BucketCount]) -> (NaiveDate, NaiveDate) {
    let mut dates = data.iter().filter_map(|item| parse_date(&item.date));
    let Some(first) = dates.next() else {
        let last = Utc::now().date_naive();
        return (last - Duration::days(364), last);
    };
    dates.fold((first, first), |(min, max), date| {
        (min.min(date), max.max(date))
    })
}

fn render_month_separators(svg: &mut String, cells: &[Cell], options: &SvgOptions, top: u32) {
    let mut months = BTreeMap::<(i32, u32), Vec<Cell>>::new();
    for cell in cells {
        months
            .entry((cell.date.year(), cell.date.month()))
            .or_default()
            .push(*cell);
    }
    let months: Vec<_> = months.into_values().collect();
    let step = options.cell_size + options.cell_gap;

    for pair in months.windows(2) {
        let [previous, current] = pair else {
            continue;
        };
        let mut points = Vec::new();
        for row in 0..7 {
            let y = top + row * step;
            let previous_x = previous
                .iter()
                .filter(|cell| cell.y == y)
                .map(|cell| cell.x)
                .max();
            let current_x = current
                .iter()
                .filter(|cell| cell.y == y)
                .map(|cell| cell.x)
                .min();
            let x = previous_x
                .map(|x| x + options.cell_size + options.cell_gap / 2)
                .or_else(|| current_x.map(|x| x.saturating_sub(options.cell_gap / 2)));
            if let Some(x) = x {
                points.push((x, y));
            }
        }
        let Some(&(first_x, _)) = points.first() else {
            continue;
        };
        let mut path = format!("M {first_x} {top}");
        let mut previous_x = first_x;
        for (x, y) in points {
            let bend = y.saturating_sub(options.cell_gap / 2);
            let _ = write!(path, " L {previous_x} {bend} L {x} {bend}");
            previous_x = x;
        }
        let bottom = top + 6 * step + options.cell_size;
        let _ = write!(path, " L {previous_x} {bottom}");
        let _ = write!(
            svg,
            r#"<path d="{path}" fill="none" stroke="{}" stroke-opacity="0.75" stroke-width="0.5" stroke-linecap="round"/>"#,
            safe_color(&options.text_color)
        );
    }
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.get(..10)?, "%Y-%m-%d").ok()
}

fn safe_color(value: &str) -> &str {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "#(),.% ".contains(character))
    {
        value
    } else {
        "#000000"
    }
}

fn intensity_map(counts: &HashMap<NaiveDate, i64>) -> HashMap<NaiveDate, u8> {
    let mut values: Vec<_> = counts
        .values()
        .copied()
        .filter(|count| *count > 0)
        .map(|count| (count as f64 + 1.0).log10())
        .collect();
    values.sort_by(f64::total_cmp);
    let thresholds: Vec<_> = [10, 25, 40, 55, 70, 80, 88, 94]
        .into_iter()
        .map(|percentile| {
            let index = (percentile * values.len()).div_ceil(100).saturating_sub(1);
            values.get(index).copied().unwrap_or_default()
        })
        .collect();

    counts
        .iter()
        .map(|(date, count)| {
            let value = (*count as f64 + 1.0).log10();
            let level = thresholds
                .iter()
                .take_while(|threshold| value > **threshold)
                .count()
                + 1;
            (*date, level as u8)
        })
        .collect()
}

fn intensity_color(base: &str, background: &str, level: u8) -> String {
    let Some(base) = parse_oklch(base) else {
        return safe_color(base).to_owned();
    };
    let background = parse_oklch(background)
        .unwrap_or_else(|| Oklch::from_color(Srgb::new(0.92_f32, 0.93, 0.94).into_linear()));
    let dark = background.l < 0.5;
    let color = match level {
        0 => background,
        1 => background.mix(base, if dark { 0.15 } else { 0.2 }),
        2 => background.mix(base, if dark { 0.35 } else { 0.4 }),
        3 => Oklch::new(
            if dark {
                (base.l * 0.6).max(0.25)
            } else {
                (base.l * 1.5).min(0.85)
            },
            base.chroma * if dark { 0.7 } else { 0.5 },
            base.hue,
        ),
        4 => Oklch::new(
            if dark {
                (base.l * 0.8).max(0.35)
            } else {
                (base.l * 1.3).min(0.78)
            },
            base.chroma * if dark { 0.85 } else { 0.7 },
            base.hue,
        ),
        5..=7 => {
            let mid = Oklch::new(
                if dark {
                    (base.l * 0.8).max(0.35)
                } else {
                    (base.l * 1.3).min(0.78)
                },
                base.chroma * if dark { 0.85 } else { 0.7 },
                base.hue,
            );
            mid.mix(base, f32::from(level - 5) / 2.0)
        }
        8 | 9 if dark => Oklch::new(
            (base.l + if level == 8 { 0.15 } else { 0.25 }).min(0.8),
            (base.chroma * if level == 8 { 1.15 } else { 1.25 }).min(0.37),
            base.hue,
        ),
        8 | 9 => {
            let boost = if base.l > 0.75 { -0.08 } else { 0.08 };
            Oklch::new(
                (base.l + boost * if level == 8 { 1.0 } else { 1.5 }).clamp(0.4, 0.8),
                (base.chroma * if level == 8 { 1.25 } else { 1.4 }).min(0.37),
                base.hue,
            )
        }
        _ => base,
    };
    let linear: LinSrgb<f32> = LinSrgb::from_color(color);
    let encoded: Srgb<f32> = Srgb::from_linear(linear);
    let encoded = encoded.into_format::<u8>();
    format!(
        "#{:02x}{:02x}{:02x}",
        encoded.red, encoded.green, encoded.blue
    )
}

fn parse_oklch(value: &str) -> Option<Oklch> {
    let (red, green, blue) = parse_hex(value)?;
    Some(Oklch::from_color(
        Srgb::new(red, green, blue)
            .into_format::<f32>()
            .into_linear(),
    ))
}

fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.strip_prefix('#')?;
    (value.len() == 6).then_some((
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> SvgOptions {
        SvgOptions {
            cell_size: 12,
            cell_gap: 3,
            show_month_labels: true,
            show_day_labels: true,
            base_color: "#fb7185".to_owned(),
            text_color: "#57606a".to_owned(),
            cell_background: "#ebedf0".to_owned(),
        }
    }

    #[test]
    fn renders_empty_year() {
        let svg = generate_contribution_graph(&[], &options());
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.matches("<rect").count() >= 365);
    }

    #[test]
    fn renders_data_and_custom_colors() {
        let data = vec![BucketCount {
            date: "2024-01-01".to_owned(),
            count: 10,
        }];
        let mut options = options();
        options.base_color = "#ff0000".to_owned();
        let svg = generate_contribution_graph(&data, &options);
        assert!(svg.contains("2024-01-01: 10"));
        assert!(svg.contains("10 commands over 1 days"));
        assert!(svg.contains(">Less<"));
        assert!(svg.contains(">More<"));
    }
}
