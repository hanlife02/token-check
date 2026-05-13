use crate::model::{Usage, UsageEvent};
use std::collections::BTreeSet;

const TOKENS_PER_MILLION: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Cost {
    pub usd: f64,
    pub unpriced_events: usize,
    pub unpriced_tokens: u64,
}

impl Cost {
    pub fn add_assign(&mut self, other: Cost) {
        self.usd += other.usd;
        self.unpriced_events += other.unpriced_events;
        self.unpriced_tokens += other.unpriced_tokens;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenAccounting {
    CachedInputSubset,
    CachedInputSeparate,
}

#[derive(Clone, Copy, Debug)]
struct TokenPrices {
    input: f64,
    cached_input: f64,
    cache_creation_5m: f64,
    cache_creation_1h: f64,
    output: f64,
    accounting: TokenAccounting,
}

pub fn event_cost(event: &UsageEvent) -> Cost {
    let Some(prices) = prices_for_model(&event.model) else {
        if event.usage.computed_total() == 0 {
            return Cost::default();
        }
        return Cost {
            usd: 0.0,
            unpriced_events: 1,
            unpriced_tokens: event.usage.computed_total(),
        };
    };

    Cost {
        usd: prices.cost(&event.usage),
        unpriced_events: 0,
        unpriced_tokens: 0,
    }
}

pub fn unpriced_model_warnings<'a>(events: impl Iterator<Item = &'a UsageEvent>) -> Vec<String> {
    events
        .filter(|event| event.usage.computed_total() > 0)
        .filter(|event| prices_for_model(&event.model).is_none())
        .map(|event| format!("{}/{}", event.source.label(), event.model))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|model| format!("No pricing configured for {model}; omitted from dollar totals"))
        .collect()
}

impl TokenPrices {
    fn openai(input: f64, cached_input: f64, output: f64) -> Self {
        Self {
            input,
            cached_input,
            cache_creation_5m: 0.0,
            cache_creation_1h: 0.0,
            output,
            accounting: TokenAccounting::CachedInputSubset,
        }
    }

    fn anthropic(
        input: f64,
        cache_creation_5m: f64,
        cache_creation_1h: f64,
        cached_input: f64,
        output: f64,
    ) -> Self {
        Self {
            input,
            cached_input,
            cache_creation_5m,
            cache_creation_1h,
            output,
            accounting: TokenAccounting::CachedInputSeparate,
        }
    }

    fn cost(self, usage: &Usage) -> f64 {
        let input_tokens = match self.accounting {
            TokenAccounting::CachedInputSubset => usage.input.saturating_sub(usage.cached_input),
            TokenAccounting::CachedInputSeparate => usage.input,
        };
        cost_component(input_tokens, self.input)
            + cost_component(usage.cached_input, self.cached_input)
            + cost_component(usage.cache_creation_input_5m, self.cache_creation_5m)
            + cost_component(usage.cache_creation_input_1h, self.cache_creation_1h)
            + cost_component(usage.output, self.output)
    }
}

fn prices_for_model(model: &str) -> Option<TokenPrices> {
    let model = normalized_model(model);
    if model == "unknown" || model == "<synthetic>" {
        return None;
    }

    openai_prices(&model).or_else(|| anthropic_prices(&model))
}

fn openai_prices(model: &str) -> Option<TokenPrices> {
    match model {
        "gpt-5.5" => Some(TokenPrices::openai(5.00, 0.50, 30.00)),
        "gpt-5.4" => Some(TokenPrices::openai(2.50, 0.25, 15.00)),
        "gpt-5.4-mini" => Some(TokenPrices::openai(0.75, 0.075, 4.50)),
        "gpt-5.4-nano" => Some(TokenPrices::openai(0.20, 0.02, 1.25)),
        "gpt-5.3-codex" => Some(TokenPrices::openai(1.75, 0.175, 14.00)),
        _ => None,
    }
}

fn anthropic_prices(model: &str) -> Option<TokenPrices> {
    if has_all(model, &["claude", "opus", "4.7"])
        || has_all(model, &["claude", "opus", "4.6"])
        || has_all(model, &["claude", "opus", "4.5"])
    {
        return Some(TokenPrices::anthropic(5.00, 6.25, 10.00, 0.50, 25.00));
    }
    if has_all(model, &["claude", "opus", "4.1"]) || has_all(model, &["claude", "opus", "4"]) {
        return Some(TokenPrices::anthropic(15.00, 18.75, 30.00, 1.50, 75.00));
    }
    if has_all(model, &["claude", "sonnet", "4.6"])
        || has_all(model, &["claude", "sonnet", "4.5"])
        || has_all(model, &["claude", "sonnet", "4"])
        || has_all(model, &["claude", "sonnet", "3.7"])
    {
        return Some(TokenPrices::anthropic(3.00, 3.75, 6.00, 0.30, 15.00));
    }
    if has_all(model, &["claude", "haiku", "4.5"]) {
        return Some(TokenPrices::anthropic(1.00, 1.25, 2.00, 0.10, 5.00));
    }
    if has_all(model, &["claude", "haiku", "3.5"]) {
        return Some(TokenPrices::anthropic(0.80, 1.00, 1.60, 0.08, 4.00));
    }
    if has_all(model, &["claude", "haiku", "3"]) {
        return Some(TokenPrices::anthropic(0.25, 0.30, 0.50, 0.03, 1.25));
    }
    None
}

fn normalized_model(model: &str) -> String {
    model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
}

fn has_all(value: &str, needles: &[&str]) -> bool {
    needles.iter().all(|needle| value.contains(needle))
}

fn cost_component(tokens: u64, dollars_per_million: f64) -> f64 {
    tokens as f64 / TOKENS_PER_MILLION * dollars_per_million
}

#[cfg(test)]
mod tests {
    use super::event_cost;
    use crate::model::{Source, Usage, UsageEvent};

    #[test]
    fn prices_openai_cached_input_as_input_subset() {
        let cost = event_cost(&event(
            Source::Codex,
            "openai/gpt-5.4",
            Usage {
                input: 1_000_000,
                cached_input: 400_000,
                output: 200_000,
                total: 1_200_000,
                ..Usage::default()
            },
        ));

        assert_eq!(format!("{:.2}", cost.usd), "4.60");
        assert_eq!(cost.unpriced_events, 0);
    }

    #[test]
    fn prices_anthropic_cache_tokens_separately() {
        let cost = event_cost(&event(
            Source::Claude,
            "claude-sonnet-4.6",
            Usage {
                input: 1_000_000,
                cached_input: 2_000_000,
                cache_creation_input: 3_000_000,
                cache_creation_input_5m: 1_000_000,
                cache_creation_input_1h: 2_000_000,
                output: 100_000,
                total: 6_100_000,
                ..Usage::default()
            },
        ));

        assert_eq!(format!("{:.2}", cost.usd), "20.85");
        assert_eq!(cost.unpriced_events, 0);
    }

    #[test]
    fn marks_unknown_models_as_unpriced() {
        let cost = event_cost(&event(
            Source::Claude,
            "mimo-v2.5-pro",
            Usage {
                total: 123,
                ..Usage::default()
            },
        ));

        assert_eq!(cost.usd, 0.0);
        assert_eq!(cost.unpriced_events, 1);
        assert_eq!(cost.unpriced_tokens, 123);
    }

    fn event(source: Source, model: &str, usage: Usage) -> UsageEvent {
        UsageEvent {
            source,
            event_id: String::from("event"),
            session_id: String::from("session"),
            date: String::from("2026-05-13"),
            project: String::from("/tmp/project"),
            model: model.to_string(),
            usage,
        }
    }
}
