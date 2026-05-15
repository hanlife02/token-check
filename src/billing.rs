use crate::model::{Source, Usage, UsageEvent};
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
enum UsageAccounting {
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
}

#[derive(Clone, Copy, Debug)]
struct PriceRule {
    standard: TokenPrices,
    high_context: Option<(u64, TokenPrices)>,
}

pub fn event_cost(event: &UsageEvent) -> Cost {
    let accounting = usage_accounting(event.source);
    let Some(price_rule) = price_rule_for_model(&event.model) else {
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
        usd: price_rule.cost(&event.usage, accounting),
        unpriced_events: 0,
        unpriced_tokens: 0,
    }
}

pub fn unpriced_model_warnings<'a>(events: impl Iterator<Item = &'a UsageEvent>) -> Vec<String> {
    events
        .filter(|event| event.usage.computed_total() > 0)
        .filter(|event| price_rule_for_model(&event.model).is_none())
        .map(|event| format!("{}/{}", event.source.label(), event.model))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|model| format!("No pricing configured for {model}; omitted from dollar totals"))
        .collect()
}

impl TokenPrices {
    fn standard(input: f64, cached_input: f64, output: f64) -> Self {
        Self {
            input,
            cached_input,
            cache_creation_5m: 0.0,
            cache_creation_1h: 0.0,
            output,
        }
    }

    fn no_cached_discount(input: f64, output: f64) -> Self {
        Self::standard(input, input, output)
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
        }
    }

    fn cost(self, usage: &Usage, accounting: UsageAccounting) -> f64 {
        let input_tokens = match accounting {
            UsageAccounting::CachedInputSubset => usage.input.saturating_sub(usage.cached_input),
            UsageAccounting::CachedInputSeparate => usage.input,
        };
        cost_component(input_tokens, self.input)
            + cost_component(usage.cached_input, self.cached_input)
            + cost_component(usage.cache_creation_input_5m, self.cache_creation_5m)
            + cost_component(usage.cache_creation_input_1h, self.cache_creation_1h)
            + cost_component(usage.output, self.output)
    }
}

impl PriceRule {
    fn flat(prices: TokenPrices) -> Self {
        Self {
            standard: prices,
            high_context: None,
        }
    }

    fn tiered(prompt_threshold: u64, standard: TokenPrices, high_context: TokenPrices) -> Self {
        Self {
            standard,
            high_context: Some((prompt_threshold, high_context)),
        }
    }

    fn cost(self, usage: &Usage, accounting: UsageAccounting) -> f64 {
        self.prices_for_usage(usage, accounting)
            .cost(usage, accounting)
    }

    fn prices_for_usage(self, usage: &Usage, accounting: UsageAccounting) -> TokenPrices {
        if let Some((threshold, high_context)) = self.high_context {
            if prompt_tokens_for_tier(usage, accounting) > threshold {
                return high_context;
            }
        }
        self.standard
    }
}

fn usage_accounting(source: Source) -> UsageAccounting {
    match source {
        Source::Claude => UsageAccounting::CachedInputSeparate,
        Source::Codex => UsageAccounting::CachedInputSubset,
    }
}

fn prompt_tokens_for_tier(usage: &Usage, accounting: UsageAccounting) -> u64 {
    match accounting {
        UsageAccounting::CachedInputSubset => usage.input + usage.cache_creation_input,
        UsageAccounting::CachedInputSeparate => {
            usage.input + usage.cached_input + usage.cache_creation_input
        }
    }
}

fn price_rule_for_model(model: &str) -> Option<PriceRule> {
    let model = normalized_model(model);
    if model == "unknown" || model == "<synthetic>" {
        return None;
    }

    openai_prices(&model)
        .or_else(|| anthropic_prices(&model))
        .or_else(|| gemini_prices(&model))
        .or_else(|| deepseek_prices(&model))
        .or_else(|| mimo_prices(&model))
        .or_else(|| kimi_prices(&model))
        .or_else(|| moonshot_prices(&model))
}

fn openai_prices(model: &str) -> Option<PriceRule> {
    let prices = if matches_model(model, &["gpt-5.5"]) {
        TokenPrices::standard(5.00, 0.50, 30.00)
    } else if matches_model(model, &["gpt-5.4"]) {
        TokenPrices::standard(2.50, 0.25, 15.00)
    } else if matches_model(model, &["gpt-5.4-mini"]) {
        TokenPrices::standard(0.75, 0.075, 4.50)
    } else if matches_model(model, &["gpt-5.4-nano"]) {
        TokenPrices::standard(0.20, 0.02, 1.25)
    } else if matches_model(model, &["gpt-5.3-codex", "gpt-5.2", "gpt-5.2-chat-latest"]) {
        TokenPrices::standard(1.75, 0.175, 14.00)
    } else if matches_model(model, &["gpt-5.2-pro"]) {
        TokenPrices::no_cached_discount(21.00, 168.00)
    } else if matches_model(model, &["gpt-5.1", "gpt-5.1-chat-latest"]) {
        TokenPrices::standard(1.25, 0.125, 10.00)
    } else if matches_model(model, &["gpt-5-pro"]) {
        TokenPrices::no_cached_discount(15.00, 120.00)
    } else if matches_model(model, &["gpt-5-mini"]) {
        TokenPrices::standard(0.25, 0.025, 2.00)
    } else if matches_model(model, &["gpt-4.1-mini"]) {
        TokenPrices::standard(0.40, 0.10, 1.60)
    } else if matches_model(model, &["gpt-4.1-nano"]) {
        TokenPrices::standard(0.10, 0.025, 0.40)
    } else if matches_model(model, &["gpt-4.1"]) {
        TokenPrices::standard(2.00, 0.50, 8.00)
    } else if matches_model(model, &["gpt-4o-mini"]) {
        TokenPrices::standard(0.15, 0.075, 0.60)
    } else if matches_model(model, &["gpt-4o"]) {
        TokenPrices::standard(2.50, 1.25, 10.00)
    } else if matches_model(model, &["o3-deep-research"]) {
        TokenPrices::standard(10.00, 2.50, 40.00)
    } else if matches_model(model, &["o4-mini-deep-research", "o3"]) {
        TokenPrices::standard(2.00, 0.50, 8.00)
    } else if matches_model(model, &["o4-mini"]) {
        TokenPrices::standard(1.10, 0.275, 4.40)
    } else {
        return None;
    };

    Some(PriceRule::flat(prices))
}

fn anthropic_prices(model: &str) -> Option<PriceRule> {
    let prices = if is_claude_family(model, "opus")
        && has_any(
            model,
            &[
                "opus-4.7", "opus-4-7", "opus-4.6", "opus-4-6", "opus-4.5", "opus-4-5",
            ],
        ) {
        TokenPrices::anthropic(5.00, 6.25, 10.00, 0.50, 25.00)
    } else if is_claude_family(model, "opus")
        && has_any(
            model,
            &[
                "opus-4", "opus 4", "4-opus", "4.1-opus", "4-1-opus", "opus-3", "opus 3", "3-opus",
            ],
        )
    {
        TokenPrices::anthropic(15.00, 18.75, 30.00, 1.50, 75.00)
    } else if is_claude_family(model, "sonnet")
        && has_any(
            model,
            &[
                "sonnet-4",
                "sonnet 4",
                "4-sonnet",
                "sonnet-3.7",
                "sonnet-3-7",
                "3-7-sonnet",
                "sonnet-3.5",
                "sonnet-3-5",
                "3-5-sonnet",
            ],
        )
    {
        TokenPrices::anthropic(3.00, 3.75, 6.00, 0.30, 15.00)
    } else if is_claude_family(model, "haiku")
        && has_any(model, &["haiku-4.5", "haiku-4-5", "4-5-haiku"])
    {
        TokenPrices::anthropic(1.00, 1.25, 2.00, 0.10, 5.00)
    } else if is_claude_family(model, "haiku")
        && has_any(model, &["haiku-3.5", "haiku-3-5", "3-5-haiku"])
    {
        TokenPrices::anthropic(0.80, 1.00, 1.60, 0.08, 4.00)
    } else if is_claude_family(model, "haiku") && has_any(model, &["haiku-3", "haiku 3", "3-haiku"])
    {
        TokenPrices::anthropic(0.25, 0.30, 0.50, 0.03, 1.25)
    } else {
        return None;
    };

    Some(PriceRule::flat(prices))
}

fn gemini_prices(model: &str) -> Option<PriceRule> {
    if matches_model(
        model,
        &[
            "gemini-3.1-pro-preview",
            "gemini-3.1-pro-preview-customtools",
        ],
    ) {
        return Some(PriceRule::tiered(
            200_000,
            TokenPrices::standard(2.00, 0.20, 12.00),
            TokenPrices::standard(4.00, 0.40, 18.00),
        ));
    }
    if matches_model(model, &["gemini-3-flash-preview"]) {
        return Some(PriceRule::flat(TokenPrices::standard(0.50, 0.05, 3.00)));
    }
    if matches_model(
        model,
        &["gemini-3.1-flash-lite", "gemini-3.1-flash-lite-preview"],
    ) {
        return Some(PriceRule::flat(TokenPrices::standard(0.25, 0.025, 1.50)));
    }
    if matches_model(model, &["gemini-2.5-pro"]) {
        return Some(PriceRule::tiered(
            200_000,
            TokenPrices::standard(1.25, 0.125, 10.00),
            TokenPrices::standard(2.50, 0.25, 15.00),
        ));
    }
    if matches_model(
        model,
        &[
            "gemini-2.5-flash",
            "gemini-2.5-flash-image",
            "gemini-2.5-flash-native-audio-preview",
        ],
    ) {
        return Some(PriceRule::flat(TokenPrices::standard(0.30, 0.03, 2.50)));
    }
    if matches_model(
        model,
        &["gemini-2.5-flash-lite", "gemini-2.5-flash-lite-preview"],
    ) {
        return Some(PriceRule::flat(TokenPrices::standard(0.10, 0.01, 0.40)));
    }
    if matches_model(model, &["gemini-2.0-flash"]) {
        return Some(PriceRule::flat(TokenPrices::standard(0.10, 0.025, 0.40)));
    }
    if matches_model(model, &["gemini-2.0-flash-lite"]) {
        return Some(PriceRule::flat(TokenPrices::no_cached_discount(
            0.075, 0.30,
        )));
    }
    None
}

fn deepseek_prices(model: &str) -> Option<PriceRule> {
    if matches_model(model, &["deepseek-v4-pro"]) {
        return Some(PriceRule::flat(TokenPrices::standard(
            0.435, 0.003625, 0.87,
        )));
    }
    if matches_model(
        model,
        &["deepseek-v4-flash", "deepseek-chat", "deepseek-reasoner"],
    ) {
        return Some(PriceRule::flat(TokenPrices::standard(0.14, 0.0028, 0.28)));
    }
    None
}

fn mimo_prices(model: &str) -> Option<PriceRule> {
    if matches_model(model, &["mimo-v2.5-pro", "mimo-v2-pro"]) {
        return Some(PriceRule::tiered(
            256_000,
            TokenPrices::standard(1.05, 0.21, 3.15),
            TokenPrices::standard(2.10, 0.42, 6.30),
        ));
    }
    if matches_model(model, &["mimo-v2.5"]) {
        return Some(PriceRule::tiered(
            256_000,
            TokenPrices::standard(0.42, 0.08, 2.10),
            TokenPrices::standard(0.84, 0.17, 4.20),
        ));
    }
    if matches_model(model, &["mimo-v2-omni"]) {
        return Some(PriceRule::flat(TokenPrices::standard(0.42, 0.08, 2.10)));
    }
    None
}

fn kimi_prices(model: &str) -> Option<PriceRule> {
    if matches_model(model, &["kimi-k2.6"]) {
        return Some(PriceRule::flat(TokenPrices::standard(0.95, 0.16, 4.00)));
    }
    if matches_model(model, &["kimi-k2.5"]) {
        return Some(PriceRule::flat(TokenPrices::standard(0.60, 0.10, 3.00)));
    }
    if matches_model(
        model,
        &[
            "kimi-k2-0905-preview",
            "kimi-k2-0711-preview",
            "kimi-k2-thinking",
        ],
    ) {
        return Some(PriceRule::flat(TokenPrices::standard(0.60, 0.15, 2.50)));
    }
    if matches_model(model, &["kimi-k2-turbo-preview", "kimi-k2-thinking-turbo"]) {
        return Some(PriceRule::flat(TokenPrices::standard(1.15, 0.15, 8.00)));
    }
    None
}

fn moonshot_prices(model: &str) -> Option<PriceRule> {
    if matches_model(model, &["moonshot-v1-8k", "moonshot-v1-8k-vision-preview"]) {
        return Some(PriceRule::flat(TokenPrices::no_cached_discount(0.20, 2.00)));
    }
    if matches_model(
        model,
        &["moonshot-v1-32k", "moonshot-v1-32k-vision-preview"],
    ) {
        return Some(PriceRule::flat(TokenPrices::no_cached_discount(1.00, 3.00)));
    }
    if matches_model(
        model,
        &["moonshot-v1-128k", "moonshot-v1-128k-vision-preview"],
    ) {
        return Some(PriceRule::flat(TokenPrices::no_cached_discount(2.00, 5.00)));
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

fn matches_model(model: &str, bases: &[&str]) -> bool {
    bases
        .iter()
        .any(|base| model == *base || model.strip_prefix(base).is_some_and(is_snapshot_suffix))
}

fn is_snapshot_suffix(value: &str) -> bool {
    let Some(value) = value.strip_prefix('-') else {
        return false;
    };
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next(), chars.next(), chars.next()),
        (Some('2'), Some('0'), Some(_), Some(_))
    )
}

fn is_claude_family(model: &str, family: &str) -> bool {
    model.contains("claude") && model.contains(family)
}

fn has_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn cost_component(tokens: u64, dollars_per_million: f64) -> f64 {
    tokens as f64 / TOKENS_PER_MILLION * dollars_per_million
}

#[cfg(test)]
mod tests {
    use super::{event_cost, unpriced_model_warnings};
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
            "unknown-paid-model",
            Usage {
                total: 123,
                ..Usage::default()
            },
        ));

        assert_eq!(cost.usd, 0.0);
        assert_eq!(cost.unpriced_events, 1);
        assert_eq!(cost.unpriced_tokens, 123);
    }

    #[test]
    fn prices_openai_snapshots_and_claude_style_cache_shapes() {
        let codex_cost = event_cost(&event(
            Source::Codex,
            "gpt-4.1-mini-2025-04-14",
            Usage {
                input: 1_000_000,
                cached_input: 100_000,
                output: 1_000_000,
                total: 2_000_000,
                ..Usage::default()
            },
        ));
        let claude_cost = event_cost(&event(
            Source::Claude,
            "openai/gpt-4.1-mini",
            Usage {
                input: 1_000_000,
                cached_input: 100_000,
                output: 1_000_000,
                total: 2_100_000,
                ..Usage::default()
            },
        ));

        assert_eq!(format!("{:.2}", codex_cost.usd), "1.97");
        assert_eq!(format!("{:.2}", claude_cost.usd), "2.01");
    }

    #[test]
    fn prices_common_claude_hyphenated_versions() {
        let cost = event_cost(&event(
            Source::Claude,
            "claude-3-5-haiku-20241022",
            Usage {
                input: 1_000_000,
                cached_input: 1_000_000,
                output: 1_000_000,
                total: 3_000_000,
                ..Usage::default()
            },
        ));

        assert_eq!(format!("{:.2}", cost.usd), "4.88");
        assert_eq!(cost.unpriced_events, 0);
    }

    #[test]
    fn prices_deepseek_and_mimo_aliases() {
        let deepseek_cost = event_cost(&event(
            Source::Claude,
            "deepseek-v4-pro",
            Usage {
                input: 1_000_000,
                cached_input: 1_000_000,
                output: 1_000_000,
                total: 3_000_000,
                ..Usage::default()
            },
        ));
        let mimo_cost = event_cost(&event(
            Source::Claude,
            "mimo-v2.5-pro",
            Usage {
                input: 100_000,
                cached_input: 100_000,
                output: 100_000,
                total: 300_000,
                ..Usage::default()
            },
        ));
        let high_context_mimo_cost = event_cost(&event(
            Source::Claude,
            "mimo-v2.5-pro",
            Usage {
                input: 300_000,
                output: 1_000_000,
                total: 1_300_000,
                ..Usage::default()
            },
        ));

        assert_eq!(format!("{:.4}", deepseek_cost.usd), "1.3086");
        assert_eq!(format!("{:.3}", mimo_cost.usd), "0.441");
        assert_eq!(format!("{:.2}", high_context_mimo_cost.usd), "6.93");
    }

    #[test]
    fn prices_gemini_kimi_and_moonshot_models() {
        let gemini_cost = event_cost(&event(
            Source::Codex,
            "google/gemini-2.5-flash",
            Usage {
                input: 1_000_000,
                cached_input: 200_000,
                output: 1_000_000,
                total: 2_000_000,
                ..Usage::default()
            },
        ));
        let kimi_cost = event_cost(&event(
            Source::Claude,
            "moonshot/kimi-k2.6",
            Usage {
                input: 1_000_000,
                cached_input: 1_000_000,
                output: 1_000_000,
                total: 3_000_000,
                ..Usage::default()
            },
        ));
        let moonshot_cost = event_cost(&event(
            Source::Claude,
            "moonshot-v1-32k",
            Usage {
                input: 1_000_000,
                cached_input: 1_000_000,
                output: 1_000_000,
                total: 3_000_000,
                ..Usage::default()
            },
        ));

        assert_eq!(format!("{:.3}", gemini_cost.usd), "2.746");
        assert_eq!(format!("{:.2}", kimi_cost.usd), "5.11");
        assert_eq!(format!("{:.2}", moonshot_cost.usd), "5.00");
    }

    #[test]
    fn only_warns_for_models_without_configured_prices() {
        let events = vec![
            event(
                Source::Claude,
                "deepseek-v4-pro",
                Usage {
                    total: 123,
                    ..Usage::default()
                },
            ),
            event(
                Source::Claude,
                "mimo-v2.5-pro",
                Usage {
                    total: 456,
                    ..Usage::default()
                },
            ),
            event(
                Source::Codex,
                "google/gemini-2.5-flash",
                Usage {
                    total: 456,
                    ..Usage::default()
                },
            ),
            event(
                Source::Claude,
                "moonshot/kimi-k2.6",
                Usage {
                    total: 456,
                    ..Usage::default()
                },
            ),
            event(
                Source::Claude,
                "unknown-paid-model",
                Usage {
                    total: 789,
                    ..Usage::default()
                },
            ),
        ];

        assert_eq!(
            unpriced_model_warnings(events.iter()),
            vec![
                "No pricing configured for claude/unknown-paid-model; omitted from dollar totals"
                    .to_string()
            ]
        );
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
