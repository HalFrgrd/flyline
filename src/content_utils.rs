use ratatui::prelude::*;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn vec_spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|s| s.width()).sum()
}

pub fn take_prefix_of_spans(spans: &[Span<'static>], mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return vec![];
    }

    let mut out: Vec<Span<'static>> = Vec::new();

    for span in spans {
        if n == 0 {
            break;
        }
        let span_width = span.width();
        if span_width <= n {
            out.push(span.clone());
            n -= span_width;
        } else {
            span.content
                .graphemes(true)
                .take_while(|g| {
                    let g_width = g.width();
                    if g_width <= n {
                        n -= g_width;
                        true
                    } else {
                        false
                    }
                })
                .for_each(|g| out.push(Span::styled(g.to_owned(), span.style)));

            break;
        }
    }
    out
}

pub fn take_suffix_of_spans(spans: &[Span<'static>], mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return vec![];
    }

    let mut out: Vec<Span<'static>> = Vec::new();

    for span in spans.iter().rev() {
        if n == 0 {
            break;
        }
        let span_width = span.width();
        if span_width <= n {
            out.push(span.clone());
            n -= span_width;
        } else {
            span.content
                .graphemes(true)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .take_while(|g| {
                    let g_width = g.width();
                    if g_width <= n {
                        n -= g_width;
                        true
                    } else {
                        false
                    }
                })
                .for_each(|g| out.push(Span::styled(g.to_owned(), span.style)));

            break;
        }
    }
    out.reverse();
    out
}
