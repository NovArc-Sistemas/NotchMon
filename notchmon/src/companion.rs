//! The companion: PokeTokenBar's CompanionStore, in Rust. Tokens spent raise an egg into a Pokémon
//! that evolves through its real line and graduates into the Pokédex; a fresh egg follows.
//!
//! Balance (PokeTokenBar, unchanged): hatch at 5M tokens; graduation totals 750M / 1.875B / 3B / 6B by
//! rarity, split over the forms so later forms cost more; a line already graduated grows twice as
//! fast; shiny 1 in 64 (1 in 48 with the charm); Ditto disguise 1 in 128 on a common line with two or
//! more forms; Rare Candy +100M, one per 5-hour window that fills, five per weekly one; the shop sells
//! on tokens already spent. Growth and prices scale by two independent difficulty multipliers.
//!
//! Pure logic here: the engine takes a `Provider` for PokéAPI and an RNG, so the tests below run
//! against a fake with a seeded generator. `start` runs the worker that feeds it every minute.

use crate::pokeapi::{BaseSpecies, Details, EvoLine, EvoNode, Provider, Rarity, DITTO, MAX_SPECIES};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

pub const EGG_HATCH: u64 = 5_000_000;
pub const REPEAT_BOOST: u64 = 2;
pub const CANDY_XP: u64 = 100_000_000;
pub const CANDY_PRICE: u64 = 500_000_000;
pub const WEEKLY_CANDIES: u32 = 5;
pub const MINT_PRICE: u64 = 100_000_000;
pub const CHARM_PRICE: u64 = 3_000_000_000;
pub const EGG_PRICE: u64 = 1_000_000_000;
pub const SHINY_DENOM: u64 = 64;
pub const SHINY_DENOM_CHARM: u64 = 48;
pub const DITTO_DENOM: u64 = 128;
pub const DIFFICULTY_RANGE: (f64, f64) = (0.1, 2.0);
/// How long the level-up copy (hatched / evolved / graduated) stays up
const EVENT_SECS: u64 = 6;

pub fn graduation_total(r: Rarity) -> u64 {
    match r {
        Rarity::Common => 750_000_000,
        Rarity::Uncommon => 1_875_000_000,
        Rarity::Rare => 3_000_000_000,
        Rarity::Legendary => 6_000_000_000,
    }
}

/// Tokens the form at `stage` (0-based) needs before the next one, out of `forms`: T·i / (k(k+1)/2)
pub fn phase_threshold(r: Rarity, forms: usize, stage: usize, boost: u64) -> u64 {
    let k = forms.max(1) as f64;
    let i = (stage + 1) as f64;
    let denom = k * (k + 1.0) / 2.0;
    let t = (graduation_total(r) as f64 * i / denom).round();
    ((t / boost.max(1) as f64).round() as u64).max(1)
}

pub fn clamp_difficulty(v: f64) -> f64 {
    if !v.is_finite() {
        1.0
    } else {
        v.clamp(DIFFICULTY_RANGE.0, DIFFICULTY_RANGE.1)
    }
}

fn scaled(base: u64, d: f64) -> u64 {
    (base as f64 * clamp_difficulty(d)).round() as u64
}

pub fn egg_price(tier: Option<Rarity>) -> u64 {
    match tier {
        None => EGG_PRICE,
        Some(t) => (EGG_PRICE as f64 * graduation_total(t) as f64 / graduation_total(Rarity::Common) as f64).round() as u64,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Item {
    RareCandy,
    Mint,
    ShinyCharm,
}

impl Item {
    pub const ALL: [Item; 3] = [Item::RareCandy, Item::Mint, Item::ShinyCharm];
    pub fn key(self) -> &'static str {
        match self {
            Item::RareCandy => "rareCandy",
            Item::Mint => "mint",
            Item::ShinyCharm => "shinyCharm",
        }
    }
    pub fn from_key(k: &str) -> Option<Item> {
        Item::ALL.into_iter().find(|i| i.key() == k)
    }
    pub fn price(self) -> u64 {
        match self {
            Item::RareCandy => CANDY_PRICE,
            Item::Mint => MINT_PRICE,
            Item::ShinyCharm => CHARM_PRICE,
        }
    }
    /// Owned rather than used: bought once, works while held
    pub fn passive(self) -> bool {
        self == Item::ShinyCharm
    }
}

pub const NATURES: [&str; 25] = [
    "hardy", "lonely", "brave", "adamant", "naughty", "bold", "docile", "relaxed", "impish", "lax", "timid", "hasty", "serious", "jolly", "naive", "modest", "mild", "quiet", "bashful", "rash", "calm", "gentle", "sassy", "careful", "quirky",
];

/// Main-series nature modifier for a stat name
pub fn nature_modifier(nature: &str, stat: &str) -> f64 {
    let pair: Option<(&str, &str)> = match nature {
        "lonely" => Some(("attack", "defense")),
        "brave" => Some(("attack", "speed")),
        "adamant" => Some(("attack", "special-attack")),
        "naughty" => Some(("attack", "special-defense")),
        "bold" => Some(("defense", "attack")),
        "relaxed" => Some(("defense", "speed")),
        "impish" => Some(("defense", "special-attack")),
        "lax" => Some(("defense", "special-defense")),
        "timid" => Some(("speed", "attack")),
        "hasty" => Some(("speed", "defense")),
        "jolly" => Some(("speed", "special-attack")),
        "naive" => Some(("speed", "special-defense")),
        "modest" => Some(("special-attack", "attack")),
        "mild" => Some(("special-attack", "defense")),
        "quiet" => Some(("special-attack", "speed")),
        "rash" => Some(("special-attack", "special-defense")),
        "calm" => Some(("special-defense", "attack")),
        "gentle" => Some(("special-defense", "defense")),
        "sassy" => Some(("special-defense", "speed")),
        "careful" => Some(("special-defense", "special-attack")),
        _ => None,
    };
    match pair {
        Some((up, _)) if up == stat => 1.1,
        Some((_, down)) if down == stat => 0.9,
        _ => 1.0,
    }
}

// ---------------- RNG ----------------

/// splitmix64: deterministic, seedable, good enough for a game
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }
    pub fn from_time() -> Rng {
        Rng::new(chrono::Utc::now().timestamp_nanos_opt().unwrap_or(1) as u64 ^ std::process::id() as u64)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// ---------------- the individual ----------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Ivs {
    pub hp: u32,
    pub attack: u32,
    pub defense: u32,
    #[serde(rename = "specialAttack")]
    pub special_attack: u32,
    #[serde(rename = "specialDefense")]
    pub special_defense: u32,
    pub speed: u32,
}

impl Ivs {
    pub fn get(&self, stat: &str) -> u32 {
        match stat {
            "hp" => self.hp,
            "attack" => self.attack,
            "defense" => self.defense,
            "special-attack" => self.special_attack,
            "special-defense" => self.special_defense,
            "speed" => self.speed,
            _ => 0,
        }
    }
}

/// What makes one caught Pokémon different from another
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub seed: u64,
    /// "male" | "female" | "genderless"; empty until details are known
    #[serde(default)]
    pub gender: String,
    pub ivs: Ivs,
    #[serde(default)]
    pub ability: String,
    #[serde(default)]
    pub ability_hidden: bool,
    pub level: u32,
    /// growth earned, in standard-balance tokens
    pub growth: u64,
    #[serde(default)]
    pub moves: Vec<(String, u32)>,
}

impl Profile {
    pub fn generate(seed: u64) -> Profile {
        let mut r = Rng::new(seed);
        let mut iv = || (r.next() % 32) as u32;
        Profile { seed, gender: String::new(), ivs: Ivs { hp: iv(), attack: iv(), defense: iv(), special_attack: iv(), special_defense: iv(), speed: iv() }, ability: String::new(), ability_hidden: false, level: 5, growth: 0, moves: vec![] }
    }

    /// Gender, ability and moves once the species' details are known (idempotent for the rolls)
    pub fn enrich(&mut self, d: &Details) {
        let mut r = Rng::new(self.seed ^ 0xA11B_1E5D_9EED);
        if self.gender.is_empty() {
            self.gender = if d.gender_rate < 0 {
                "genderless".into()
            } else if (r.next() % 8) < d.gender_rate as u64 {
                "female".into()
            } else {
                "male".into()
            };
        }
        if self.ability.is_empty() {
            let normal: Vec<_> = d.abilities.iter().filter(|a| !a.is_hidden).collect();
            let hidden: Vec<_> = d.abilities.iter().filter(|a| a.is_hidden).collect();
            let pick = if !hidden.is_empty() && r.next() % 128 == 0 {
                Some(hidden[(r.next() % hidden.len() as u64) as usize])
            } else if !normal.is_empty() {
                Some(normal[(r.next() % normal.len() as u64) as usize])
            } else {
                d.abilities.first()
            };
            if let Some(a) = pick {
                self.ability = a.name.clone();
                self.ability_hidden = a.is_hidden;
            }
        }
        // The four most recently learned level-up moves
        let known = d.moves_through(self.level);
        self.moves = known.iter().rev().take(4).rev().map(|m| (m.name.clone(), m.level)).collect();
    }

    /// Growth never goes backwards; level 5 → 100 over the graduation total
    pub fn advance(&mut self, to: u64, r: Rarity) {
        self.growth = self.growth.max(to);
        let progress = (self.growth as f64 / graduation_total(r) as f64).min(1.0);
        self.level = self.level.max(5 + (progress * 95.0).floor() as u32).min(100);
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stat {
    pub name: String,
    pub base: u32,
    pub iv: u32,
    pub value: u32,
}

pub fn computed_stats(d: &Details, p: &Profile, nature: &str) -> Vec<Stat> {
    ["hp", "attack", "defense", "special-attack", "special-defense", "speed"]
        .iter()
        .filter_map(|name| {
            let base = *d.stats.get(*name)?;
            let iv = p.ivs.get(name);
            let l = p.level;
            let value = if *name == "hp" {
                (2 * base + iv) * l / 100 + l + 10
            } else {
                let neutral = (2 * base + iv) * l / 100 + 5;
                (neutral as f64 * nature_modifier(nature, name)).floor() as u32
            };
            Some(Stat { name: name.to_string(), base, iv, value })
        })
        .collect()
}

// ---------------- persisted state ----------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mon {
    pub base_id: u32,
    /// the path realised so far, base first
    pub path: Vec<u32>,
    /// the whole planned path (branches chosen at hatch)
    pub planned: Vec<u32>,
    pub stage: usize,
    pub used_at_stage: u64,
    pub rarity: Rarity,
    pub total_forms: usize,
    pub is_shiny: bool,
    pub nature: String,
    pub profile: Profile,
    pub growth_boost: bool,
    /// Some(id) = a Ditto disguised as that species; revealed at the first evolution threshold
    #[serde(default)]
    pub ditto_disguise: Option<u32>,
    #[serde(default)]
    pub ditto_revealed: bool,
    /// species id → language → name, so the UI needs no fetch
    #[serde(default)]
    pub names: BTreeMap<u32, BTreeMap<String, String>>,
    /// ms epoch of the hatch
    #[serde(default)]
    pub hatched_at: u64,
}

impl Mon {
    pub fn current_id(&self) -> u32 {
        self.path.get(self.stage.min(self.path.len().saturating_sub(1))).copied().unwrap_or(self.base_id)
    }
    pub fn threshold(&self, difficulty: f64) -> u64 {
        scaled(phase_threshold(self.rarity, self.total_forms, self.stage, if self.growth_boost { REPEAT_BOOST } else { 1 }), difficulty)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DexEntry {
    pub id: String,
    pub base_id: u32,
    pub final_id: u32,
    pub chain: Vec<u32>,
    pub rarity: Rarity,
    pub caught_at: u64,
    pub is_shiny: bool,
    pub nature: String,
    pub profile: Profile,
    #[serde(default)]
    pub names: BTreeMap<u32, BTreeMap<String, String>>,
    /// set when the companion was sent off with a bought egg rather than graduated
    #[serde(default)]
    pub released_at: Option<u64>,
    #[serde(default)]
    pub hatched_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub baseline_set: bool,
    pub used_since_install: u64,
    pub spent_tokens: u64,
    pub egg_usage: u64,
    pub egg_tier: Option<Rarity>,
    pub pending_hatch: Option<u32>,
    /// provider → tokens already credited today
    pub claimed_today: Option<BTreeMap<String, u64>>,
    pub last_date: String,
    pub active: Option<Mon>,
    pub representative: Option<u32>,
    pub dex: Vec<DexEntry>,
    /// "base:final" pairs graduated, for branch variety and the repeat boost
    pub collected_finals: BTreeSet<String>,
    pub inventory: BTreeMap<String, u32>,
    /// candy window key → 1 while it sits at 100 %
    pub candy_granted: BTreeMap<String, u32>,
    pub candy_seeded: bool,
    pub growth_difficulty: f64,
    pub shop_difficulty: f64,
    pub rng: Rng,
}

impl Default for State {
    fn default() -> Self {
        State {
            baseline_set: false,
            used_since_install: 0,
            spent_tokens: 0,
            egg_usage: 0,
            egg_tier: None,
            pending_hatch: None,
            claimed_today: None,
            last_date: String::new(),
            active: None,
            representative: None,
            dex: Vec::new(),
            collected_finals: BTreeSet::new(),
            inventory: BTreeMap::new(),
            candy_granted: BTreeMap::new(),
            candy_seeded: false,
            growth_difficulty: 1.0,
            shop_difficulty: 1.0,
            rng: Rng::from_time(),
        }
    }
}

/// One limit window that can pay a Rare Candy
#[derive(Debug, Clone, PartialEq)]
pub struct CandyWindow {
    pub key: String,
    pub name: String,
    pub weekly: bool,
    /// 0–100+
    pub utilization: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandyGrant {
    pub window: String,
    pub count: u32,
}

/// Something worth telling the user (notification, bubble, celebration)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    Hatched { id: u32, name: String, shiny: bool },
    Evolved { id: u32, name: String },
    Graduated { id: u32, name: String },
    DittoRevealed { shiny: bool },
    Candy { count: u32, window: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayState {
    Egg,
    Idle,
    Working,
    Focus,
    Tired,
    Sleep,
    LevelUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurnTier {
    Idle,
    Normal,
    Fast,
    Blazing,
}

impl BurnTier {
    pub fn of(tokens_per_min: f64) -> BurnTier {
        if tokens_per_min <= 1_000.0 {
            BurnTier::Idle
        } else if tokens_per_min < 100_000.0 {
            BurnTier::Normal
        } else if tokens_per_min < 400_000.0 {
            BurnTier::Fast
        } else {
            BurnTier::Blazing
        }
    }
}

// ---------------- the engine ----------------

pub struct Engine {
    pub state: State,
    pub provider: Box<dyn Provider>,
    pub lang: String,
    /// the active line, loaded lazily (a restart, or offline at hatch)
    pub line: Option<EvoLine>,
    pub details: BTreeMap<u32, Details>,
    pub display: DisplayState,
    pub events: Vec<Event>,
    /// ms epoch until which the level-up copy shows
    pub event_until: u64,
    pub last_event: Option<Event>,
    pub dirty: bool,
    now_ms: Box<dyn Fn() -> u64 + Send + Sync>,
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn state_path() -> PathBuf {
    crate::config::config_path().with_file_name("companion.json")
}

pub fn load_state() -> State {
    let raw = std::fs::read_to_string(state_path()).ok();
    match raw.as_deref().map(serde_json::from_str::<State>) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            crate::applog(&format!("companion: state unreadable ({e}); kept aside as companion.json.corrupt"));
            let _ = std::fs::copy(state_path(), state_path().with_extension("json.corrupt"));
            State::default()
        }
        None => State::default(),
    }
}

impl Engine {
    pub fn new(state: State, provider: Box<dyn Provider>, lang: &str) -> Engine {
        let display = if state.active.is_some() { DisplayState::Idle } else { DisplayState::Egg };
        Engine { state, provider, lang: lang.to_string(), line: None, details: BTreeMap::new(), display, events: Vec::new(), event_until: 0, last_event: None, dirty: false, now_ms: Box::new(now_ms) }
    }

    #[cfg(test)]
    fn with_clock(mut self, f: impl Fn() -> u64 + Send + Sync + 'static) -> Engine {
        self.now_ms = Box::new(f);
        self
    }

    fn now(&self) -> u64 {
        (self.now_ms)()
    }

    pub fn save(&mut self) {
        self.dirty = false;
        if let Ok(t) = serde_json::to_string_pretty(&self.state) {
            let tmp = state_path().with_extension("json.tmp");
            if std::fs::write(&tmp, t).is_ok() {
                let _ = std::fs::rename(&tmp, state_path());
            }
        }
    }

    fn push(&mut self, e: Event) {
        self.last_event = Some(e.clone());
        self.events.push(e);
    }

    // ---- derived ----

    pub fn egg_threshold(&self) -> u64 {
        scaled(EGG_HATCH, self.state.growth_difficulty)
    }
    pub fn threshold(&self) -> u64 {
        self.state.active.as_ref().map(|a| a.threshold(self.state.growth_difficulty)).unwrap_or(self.egg_threshold())
    }
    pub fn available_tokens(&self) -> u64 {
        self.state.used_since_install.saturating_sub(self.state.spent_tokens)
    }
    pub fn item_count(&self, i: Item) -> u32 {
        self.state.inventory.get(i.key()).copied().unwrap_or(0)
    }
    pub fn price(&self, i: Item) -> u64 {
        scaled(i.price(), self.state.shop_difficulty)
    }
    pub fn egg_price(&self, tier: Option<Rarity>) -> u64 {
        scaled(egg_price(tier), self.state.shop_difficulty)
    }
    pub fn owns_charm(&self) -> bool {
        self.item_count(Item::ShinyCharm) > 0
    }
    pub fn name_of(&self, id: u32) -> String {
        if let Some(l) = &self.line {
            if l.names.contains_key(&id) {
                return l.name(id, &self.lang);
            }
        }
        for src in self.state.active.iter().map(|a| &a.names).chain(self.state.dex.iter().map(|d| &d.names)) {
            if let Some(n) = src.get(&id) {
                let order: &[&str] = if self.lang == "pt" { &["pt-br", "pt", "en"] } else { &[self.lang.as_str(), "en"] };
                for l in order {
                    if let Some(s) = n.get(*l) {
                        return s.clone();
                    }
                }
            }
        }
        format!("#{id}")
    }

    // ---- usage ----

    /// Feeds the day's per-provider totals. Only growth since the last observation counts; a
    /// provider seen for the first time seeds its baseline (no retroactive credit); a drop rebases.
    pub fn update(&mut self, today: &BTreeMap<String, u64>, date: &str, burn: BurnTier, limit_warning: bool, has_data: bool) {
        let has_current = has_data && !today.is_empty();
        if !self.state.baseline_set {
            if !has_current {
                self.display = if self.state.active.is_none() { DisplayState::Egg } else { DisplayState::Idle };
                return;
            }
            self.state.baseline_set = true;
            self.state.claimed_today = Some(today.clone());
            self.state.last_date = date.to_string();
            self.dirty = true;
        } else if has_current {
            let mut delta: u64 = 0;
            if self.state.claimed_today.is_none() {
                self.state.claimed_today = Some(today.clone());
                self.state.last_date = date.to_string();
            } else if date != self.state.last_date {
                // A new day: everything reported so far today is new
                self.state.last_date = date.to_string();
                let mut ledger: BTreeMap<String, u64> = self.state.claimed_today.take().unwrap_or_default().keys().map(|k| (k.clone(), 0)).collect();
                for (p, cur) in today {
                    ledger.insert(p.clone(), *cur);
                    delta += cur;
                }
                self.state.claimed_today = Some(ledger);
            } else {
                let mut ledger = self.state.claimed_today.take().unwrap_or_default();
                for (p, cur) in today {
                    match ledger.get(p) {
                        None => {
                            ledger.insert(p.clone(), *cur);
                        }
                        Some(prev) if cur < prev => {
                            ledger.insert(p.clone(), *cur);
                        }
                        Some(prev) => {
                            delta += cur - prev;
                            ledger.insert(p.clone(), *cur);
                        }
                    }
                }
                self.state.claimed_today = Some(ledger);
            }
            if delta > 0 {
                self.credit(delta);
            }
            self.dirty = true;
        }
        if self.state.active.is_none() && self.state.egg_usage >= self.egg_threshold() {
            self.hatch_if_ready();
        }
        if self.state.active.is_some() && self.line.is_none() {
            self.load_line();
        }
        let today_total: u64 = today.values().sum();
        self.display = self.compute_state(burn, limit_warning, has_data, today_total);
    }

    fn credit(&mut self, delta: u64) {
        self.state.used_since_install += delta;
        if self.state.active.is_none() {
            self.state.egg_usage += delta;
        } else {
            self.apply_usage(delta);
        }
    }

    fn compute_state(&self, burn: BurnTier, limit_warning: bool, has_data: bool, today: u64) -> DisplayState {
        if self.state.active.is_none() {
            return DisplayState::Egg;
        }
        if self.now() < self.event_until {
            return DisplayState::LevelUp;
        }
        if limit_warning {
            return DisplayState::Tired;
        }
        if !has_data || today == 0 {
            return DisplayState::Sleep;
        }
        match burn {
            BurnTier::Idle => DisplayState::Idle,
            BurnTier::Normal => DisplayState::Working,
            _ => DisplayState::Focus,
        }
    }

    /// Tokens applied to the active Pokémon: evolutions and graduation fall out of the thresholds.
    /// Usage is always banked, even with the line unloaded; only the evolution check waits.
    pub fn apply_usage(&mut self, delta: u64) {
        let Some(a) = self.state.active.as_mut() else { return };
        a.used_at_stage += delta;
        let (r, g) = (a.rarity, a.profile.growth + delta);
        a.profile.advance(g, r);
        self.dirty = true;
        if self.line.is_none() {
            return;
        }
        for _ in 0..50 {
            let Some(a) = self.state.active.clone() else { break };
            let thr = a.threshold(self.state.growth_difficulty);
            if a.used_at_stage < thr {
                break;
            }
            if a.ditto_disguise.is_some() && !a.ditto_revealed {
                self.reveal_ditto();
                continue;
            }
            let Some(line) = self.line.clone() else { break };
            let Some(node) = line.tree.find(a.current_id()) else { break };
            if node.children.is_empty() {
                self.graduate();
                break;
            }
            let next_index = a.stage + 1;
            let next = match a.planned.get(next_index).and_then(|id| node.children.iter().find(|c| c.id == *id)) {
                Some(n) => n.clone(),
                None => {
                    let n = self.pick_child(node, a.base_id);
                    let route = std::iter::once(node.id).chain(self.plan_from(&n, a.base_id)).collect::<Vec<_>>();
                    let prefix: Vec<u32> = a.path.iter().take(a.stage + 1).copied().collect();
                    let repaired = if route.first() == prefix.last() { prefix.into_iter().chain(route.into_iter().skip(1)).collect() } else { prefix };
                    let am = self.state.active.as_mut().unwrap();
                    am.planned = repaired;
                    am.total_forms = am.planned.len();
                    n
                }
            };
            let am = self.state.active.as_mut().unwrap();
            am.path.truncate(a.stage + 1);
            am.path.push(next.id);
            am.stage += 1;
            am.used_at_stage = a.used_at_stage - thr;
            let name = line.name(next.id, &self.lang);
            self.event_until = self.now() + EVENT_SECS * 1000;
            self.push(Event::Evolved { id: next.id, name });
            self.enrich_profile(next.id);
        }
    }

    fn pick_child(&mut self, node: &EvoNode, base_id: u32) -> EvoNode {
        let fresh: Vec<&EvoNode> = node.children.iter().filter(|c| c.final_ids().iter().any(|f| !self.state.collected_finals.contains(&format!("{base_id}:{f}")))).collect();
        let pool: Vec<&EvoNode> = if fresh.is_empty() { node.children.iter().collect() } else { fresh };
        pool[(self.state.rng.next() % pool.len() as u64) as usize].clone()
    }

    fn plan_from(&mut self, root: &EvoNode, base_id: u32) -> Vec<u32> {
        let mut plan = vec![root.id];
        let mut node = root.clone();
        while !node.children.is_empty() {
            let next = self.pick_child(&node, base_id);
            plan.push(next.id);
            node = next;
        }
        plan
    }

    /// Gender, ability and moves for the current form; fetches the details when they are not cached
    /// yet (the provider keeps them on disk, so this is one request per species, ever)
    fn enrich_profile(&mut self, id: u32) {
        if !self.details.contains_key(&id) {
            if let Ok(d) = self.provider.details(id) {
                self.details.insert(id, d);
            }
        }
        if let Some(d) = self.details.get(&id).cloned() {
            if let Some(a) = self.state.active.as_mut() {
                a.profile.enrich(&d);
                self.dirty = true;
            }
        }
    }

    /// Details for the profile page, cached in memory (and on disk by the provider)
    pub fn ensure_details(&mut self, id: u32) -> Option<Details> {
        if let Some(d) = self.details.get(&id) {
            return Some(d.clone());
        }
        let d = self.provider.details(id).ok()?;
        self.details.insert(id, d.clone());
        if self.state.active.as_ref().map(|a| a.current_id()) == Some(id) {
            self.enrich_profile(id);
        }
        Some(d)
    }

    fn graduate(&mut self) {
        let Some(mut a) = self.state.active.take() else { return };
        a.profile.advance(graduation_total(a.rarity), a.rarity);
        let final_id = a.current_id();
        self.state.collected_finals.insert(format!("{}:{}", a.base_id, final_id));
        let names = a.names.clone();
        let name = self.line.as_ref().map(|l| l.name(final_id, &self.lang)).unwrap_or_else(|| format!("#{final_id}"));
        self.state.dex.push(DexEntry { id: format!("{:x}", self.state.rng.next()), base_id: a.base_id, final_id, chain: a.path.clone(), rarity: a.rarity, caught_at: self.now(), is_shiny: a.is_shiny, nature: a.nature.clone(), profile: a.profile.clone(), names, released_at: None, hatched_at: a.hatched_at });
        self.line = None;
        self.state.egg_usage = 0;
        if self.state.representative.is_some_and(|r| !self.owns_species(r)) {
            self.state.representative = None;
        }
        self.event_until = self.now() + EVENT_SECS * 1000;
        self.push(Event::Graduated { id: final_id, name });
        self.dirty = true;
    }

    fn reveal_ditto(&mut self) {
        let Some(a) = self.state.active.clone() else { return };
        let Ok(line) = self.provider.line(DITTO) else {
            // Cannot reveal offline: bank the usage and try on the next tick
            return;
        };
        let thr = a.threshold(self.state.growth_difficulty);
        let am = self.state.active.as_mut().unwrap();
        let overflow = am.used_at_stage.saturating_sub(thr);
        let fraction = (am.profile.growth as f64 / graduation_total(am.rarity) as f64).min(1.0);
        am.base_id = DITTO;
        am.path = vec![DITTO];
        am.planned = vec![DITTO];
        am.stage = 0;
        am.used_at_stage = overflow;
        am.rarity = line.rarity;
        am.total_forms = 1;
        am.ditto_revealed = true;
        am.growth_boost = self.state.collected_finals.contains(&format!("{DITTO}:{DITTO}"));
        am.names = line.names.clone();
        am.profile.gender.clear();
        am.profile.ability.clear();
        am.profile.moves.clear();
        am.profile.growth = (fraction * graduation_total(line.rarity) as f64) as u64;
        let (g, r) = (am.profile.growth, line.rarity);
        am.profile.advance(g, r);
        let shiny = am.is_shiny;
        self.line = Some(line);
        self.event_until = self.now() + EVENT_SECS * 1000;
        self.push(Event::DittoRevealed { shiny });
        self.enrich_profile(DITTO);
        self.dirty = true;
    }

    // ---- hatching ----

    fn load_line(&mut self) {
        let Some(a) = &self.state.active else { return };
        if let Ok(l) = self.provider.line(a.base_id) {
            if let Some(am) = self.state.active.as_mut() {
                if am.names.is_empty() {
                    am.names = l.names.clone();
                    self.dirty = true;
                }
            }
            self.line = Some(l);
            // Thresholds may already be crossed (usage banked while offline)
            self.apply_usage(0);
        }
    }

    /// Weighted roll over every Gen I–V base species by capture rate (collected lines at half weight)
    fn choose_base(&mut self) -> Option<u32> {
        let tier = self.state.egg_tier;
        if let Ok(full) = self.provider.base_index() {
            let index: Vec<BaseSpecies> = match tier {
                Some(t) => full.into_iter().filter(|b| t.includes(b.capture_rate)).collect(),
                None => full,
            };
            if index.is_empty() {
                return None;
            }
            let weights: Vec<u64> = index.iter().map(|b| if self.collected_base(b.id) { (b.capture_rate / 2).max(1) as u64 } else { b.capture_rate.max(1) as u64 }).collect();
            let total: u64 = weights.iter().sum();
            let mut r = self.state.rng.next() % total;
            for (i, w) in weights.iter().enumerate() {
                if r < *w {
                    return Some(index[i].id);
                }
                r -= w;
            }
            return index.last().map(|b| b.id);
        }
        // The index is unreachable: rejection-sample the REST species endpoint
        for _ in 0..16 {
            let id = (self.state.rng.next() % MAX_SPECIES as u64) as u32 + 1;
            match self.provider.base_species(id) {
                Ok(Some(b)) => {
                    if tier.is_some_and(|t| !t.includes(b.capture_rate)) {
                        continue;
                    }
                    return Some(id);
                }
                Ok(None) => continue,
                Err(_) => return None,
            }
        }
        None
    }

    fn collected_base(&self, base_id: u32) -> bool {
        self.state.collected_finals.iter().any(|k| k.starts_with(&format!("{base_id}:")))
    }

    fn owns_species(&self, id: u32) -> bool {
        self.state.dex.iter().any(|d| d.chain.contains(&id)) || self.state.active.as_ref().is_some_and(|a| a.path.iter().take(a.stage + 1).any(|p| *p == id))
    }

    fn hatch_if_ready(&mut self) {
        let base = match self.state.pending_hatch {
            Some(p) => p,
            None => match self.choose_base() {
                Some(b) => {
                    self.state.pending_hatch = Some(b);
                    self.dirty = true;
                    b
                }
                None => return,
            },
        };
        self.hatch(base);
    }

    pub fn hatch(&mut self, base_id: u32) {
        let Ok(line) = self.provider.line(base_id) else { return };
        if let Some(t) = self.state.egg_tier {
            if line.rarity < t {
                // The guarantee is kept: drop this roll and try again next tick
                self.state.pending_hatch = None;
                self.dirty = true;
                return;
            }
        }
        let overflow = self.state.egg_usage.saturating_sub(self.egg_threshold());
        self.state.egg_usage = 0;
        self.state.egg_tier = None;
        self.state.pending_hatch = None;
        let shiny_roll = self.state.rng.next();
        let is_shiny = shiny_roll % if self.owns_charm() { SHINY_DENOM_CHARM } else { SHINY_DENOM } == 0;
        let nature = NATURES[(self.state.rng.next() % NATURES.len() as u64) as usize].to_string();
        let ditto = if line.rarity == Rarity::Common && line.total_forms() >= 2 && self.state.rng.next() % DITTO_DENOM == 0 { Some(line.base_id) } else { None };
        let planned = self.plan_from(&line.tree, line.base_id);
        let profile = Profile::generate(self.state.rng.next());
        let boost = self.collected_base(line.base_id);
        let name = line.name(line.base_id, &self.lang);
        self.state.active = Some(Mon { base_id: line.base_id, path: vec![line.base_id], total_forms: planned.len(), planned, stage: 0, used_at_stage: 0, rarity: line.rarity, is_shiny, nature, profile, growth_boost: boost, ditto_disguise: ditto, ditto_revealed: false, names: line.names.clone(), hatched_at: self.now() });
        self.line = Some(line);
        self.event_until = self.now() + EVENT_SECS * 1000;
        self.push(Event::Hatched { id: base_id, name, shiny: is_shiny && ditto.is_none() });
        self.enrich_profile(base_id);
        if overflow > 0 {
            self.apply_usage(overflow);
        }
        self.dirty = true;
    }

    // ---- candy ----

    /// Rising edge at 100 % pays once per window; falling below re-arms it
    pub fn grant_candies(&mut self, windows: &[CandyWindow], limits_ready: bool) -> Vec<CandyGrant> {
        if !self.state.candy_seeded {
            if !limits_ready {
                return vec![];
            }
            // Windows already full on the first run are not paid retroactively
            for w in windows.iter().filter(|w| w.utilization >= 100.0) {
                self.state.candy_granted.insert(w.key.clone(), 1);
            }
            self.state.candy_seeded = true;
            self.dirty = true;
            return vec![];
        }
        let mut grants = Vec::new();
        for w in windows {
            if w.utilization < 100.0 {
                if self.state.candy_granted.remove(&w.key).is_some() {
                    self.dirty = true;
                }
                continue;
            }
            if self.state.candy_granted.get(&w.key).copied().unwrap_or(0) >= 1 {
                continue;
            }
            self.state.candy_granted.insert(w.key.clone(), 1);
            let count = if w.weekly { WEEKLY_CANDIES } else { 1 };
            *self.state.inventory.entry(Item::RareCandy.key().into()).or_default() += count;
            self.push(Event::Candy { count, window: w.name.clone() });
            grants.push(CandyGrant { window: w.name.clone(), count });
            self.dirty = true;
        }
        grants
    }

    // ---- bag and shop ----

    pub fn can_use_candy(&self) -> bool {
        self.state.active.is_some() && self.line.is_some() && self.item_count(Item::RareCandy) > 0
    }
    pub fn use_candy(&mut self) -> bool {
        if !self.can_use_candy() {
            return false;
        }
        *self.state.inventory.get_mut(Item::RareCandy.key()).unwrap() -= 1;
        self.apply_usage(CANDY_XP);
        self.dirty = true;
        true
    }
    pub fn can_use_mint(&self) -> bool {
        self.state.active.is_some() && self.item_count(Item::Mint) > 0
    }
    pub fn use_mint(&mut self) -> Option<String> {
        if !self.can_use_mint() {
            return None;
        }
        let cur = self.state.active.as_ref()?.nature.clone();
        let pool: Vec<&str> = NATURES.iter().copied().filter(|n| *n != cur).collect();
        let new = pool[(self.state.rng.next() % pool.len() as u64) as usize].to_string();
        self.state.active.as_mut()?.nature = new.clone();
        *self.state.inventory.get_mut(Item::Mint.key()).unwrap() -= 1;
        self.dirty = true;
        Some(new)
    }
    pub fn can_buy(&self, i: Item) -> bool {
        !(i.passive() && self.item_count(i) > 0) && self.available_tokens() >= self.price(i)
    }
    pub fn buy(&mut self, i: Item) -> bool {
        if !self.can_buy(i) {
            return false;
        }
        self.state.spent_tokens += self.price(i);
        *self.state.inventory.entry(i.key().into()).or_default() += 1;
        self.dirty = true;
        true
    }
    pub fn can_buy_egg(&self, tier: Option<Rarity>) -> bool {
        tier != Some(Rarity::Legendary) && self.state.active.is_some() && self.available_tokens() >= self.egg_price(tier)
    }
    /// Sends the companion off (it stays in the Pokédex as released) and starts a fresh egg
    pub fn buy_egg(&mut self, tier: Option<Rarity>) -> bool {
        if !self.can_buy_egg(tier) {
            return false;
        }
        let price = self.egg_price(tier);
        let Some(a) = self.state.active.take() else { return false };
        self.state.spent_tokens += price;
        let reached: Vec<u32> = a.path.iter().take(a.stage + 1).copied().collect();
        self.state.dex.push(DexEntry { id: format!("{:x}", self.state.rng.next()), base_id: a.base_id, final_id: a.current_id(), chain: reached, rarity: a.rarity, caught_at: self.now(), is_shiny: a.is_shiny, nature: a.nature.clone(), profile: a.profile.clone(), names: a.names.clone(), released_at: Some(self.now()), hatched_at: a.hatched_at });
        self.line = None;
        self.state.egg_usage = 0;
        self.state.egg_tier = tier;
        self.state.pending_hatch = None;
        if self.state.representative.is_some_and(|r| !self.owns_species(r)) {
            self.state.representative = None;
        }
        self.dirty = true;
        true
    }

    pub fn set_representative(&mut self, id: Option<u32>) -> bool {
        if let Some(i) = id {
            if !self.owns_species(i) {
                return false;
            }
        }
        self.state.representative = id;
        self.dirty = true;
        true
    }

    /// Growth difficulty keeps the current progress fraction so a change never triggers an evolution
    pub fn set_difficulty(&mut self, growth: f64, shop: f64) {
        let (old, new) = (self.state.growth_difficulty, clamp_difficulty(growth));
        if (old - new).abs() > f64::EPSILON {
            let rescale = |credits: u64, base: u64| -> u64 {
                let o = scaled(base, old).max(1) as f64;
                let n = scaled(base, new).max(1) as f64;
                (credits as f64 / o * n).floor() as u64
            };
            if let Some(a) = self.state.active.as_mut() {
                let base = phase_threshold(a.rarity, a.total_forms, a.stage, if a.growth_boost { REPEAT_BOOST } else { 1 });
                a.used_at_stage = rescale(a.used_at_stage, base);
            }
            self.state.egg_usage = rescale(self.state.egg_usage, EGG_HATCH);
            self.state.growth_difficulty = new;
        }
        self.state.shop_difficulty = clamp_difficulty(shop);
        self.dirty = true;
    }
}

// ---------------- the worker ----------------

/// What the pages render. Built from the engine every tick and on every action.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub display: String,
    pub is_egg: bool,
    pub egg_usage: u64,
    pub egg_threshold: u64,
    pub egg_tier: Option<Rarity>,
    pub active: Option<Mon>,
    pub name: String,
    pub current_id: u32,
    pub stage_threshold: u64,
    pub progress: f64,
    pub tokens_to_next: u64,
    pub is_final: bool,
    /// the line as the UI draws it: [{id, state: done|current|future|mystery}]
    pub line: Vec<(u32, String)>,
    pub available_tokens: u64,
    pub used_since_install: u64,
    pub inventory: BTreeMap<String, u32>,
    pub prices: BTreeMap<String, u64>,
    pub egg_prices: BTreeMap<String, u64>,
    pub can_buy_egg: bool,
    pub can_use_candy: bool,
    pub can_use_mint: bool,
    pub representative: Option<u32>,
    pub representative_shiny: bool,
    pub dex: Vec<DexEntry>,
    pub growth_difficulty: f64,
    pub shop_difficulty: f64,
    pub last_event: Option<Event>,
    pub event_until: u64,
    pub updated_at: u64,
}

pub fn view(e: &Engine) -> View {
    let mut v = View { display: format!("{:?}", e.display).to_lowercase(), is_egg: e.state.active.is_none(), egg_usage: e.state.egg_usage, egg_threshold: e.egg_threshold(), egg_tier: e.state.egg_tier, available_tokens: e.available_tokens(), used_since_install: e.state.used_since_install, inventory: e.state.inventory.clone(), representative: e.state.representative, dex: e.state.dex.clone(), growth_difficulty: e.state.growth_difficulty, shop_difficulty: e.state.shop_difficulty, last_event: e.last_event.clone(), event_until: e.event_until, updated_at: now_ms(), can_use_candy: e.can_use_candy(), can_use_mint: e.can_use_mint(), can_buy_egg: e.can_buy_egg(None), ..Default::default() };
    if e.display == DisplayState::LevelUp {
        v.display = "levelUp".into();
    }
    for i in Item::ALL {
        v.prices.insert(i.key().into(), e.price(i));
    }
    v.egg_prices.insert("plain".into(), e.egg_price(None));
    v.egg_prices.insert("uncommon".into(), e.egg_price(Some(Rarity::Uncommon)));
    v.egg_prices.insert("rare".into(), e.egg_price(Some(Rarity::Rare)));
    if let Some(a) = &e.state.active {
        v.active = Some(a.clone());
        v.current_id = a.current_id();
        v.name = e.name_of(a.current_id());
        v.stage_threshold = a.threshold(e.state.growth_difficulty);
        v.progress = (a.used_at_stage as f64 / v.stage_threshold.max(1) as f64).min(1.0);
        v.tokens_to_next = v.stage_threshold.saturating_sub(a.used_at_stage);
        let disguised = a.ditto_disguise.is_some() && !a.ditto_revealed;
        v.is_final = !disguised && e.line.as_ref().and_then(|l| l.tree.find(a.current_id())).map(|n| n.children.is_empty()).unwrap_or(a.stage + 1 >= a.total_forms);
        for (i, id) in a.path.iter().enumerate().take(a.stage + 1) {
            v.line.push((*id, if i == a.stage { "current" } else { "done" }.into()));
        }
        if !disguised {
            // The rest of the planned path: known if its branch is settled, a mystery otherwise
            let known_branch = e.line.as_ref().and_then(|l| l.tree.find(a.current_id())).map(|n| n.children.len() <= 1).unwrap_or(true);
            for id in a.planned.iter().skip(a.stage + 1) {
                v.line.push((*id, if known_branch { "future" } else { "mystery" }.into()));
            }
        }
    }
    v.representative_shiny = v.representative.is_some_and(|r| e.state.dex.iter().any(|d| d.is_shiny && d.chain.contains(&r)) || e.state.active.as_ref().is_some_and(|a| a.is_shiny && a.path.contains(&r)));
    v
}

/// Shared engine plus the last view, for the commands
pub struct Shared {
    pub engine: std::sync::Mutex<Engine>,
}

static TICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn request_tick() {
    TICK.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn publish(app: &AppHandle, e: &mut Engine) {
    if e.dirty {
        e.save();
    }
    let v = view(e);
    for ev in e.events.drain(..) {
        let _ = app.emit("companion_event", &ev);
    }
    let _ = app.emit("companion", &v);
}

/// Every limit window the rings know about, for the candy grant
fn candy_windows(app: &AppHandle) -> (Vec<CandyWindow>, bool, f64) {
    let st = app.state::<crate::AppState>();
    let mut out = Vec::new();
    let mut ready = false;
    let mut highest: f64 = 0.0;
    let claude = st.usage.lock().unwrap().clone();
    if !claude.windows.is_empty() {
        ready = true;
    }
    for w in &claude.windows {
        if w.count.is_some() {
            continue;
        }
        highest = highest.max(w.used * 100.0);
        if w.id == "session" {
            out.push(CandyWindow { key: "claude.session".into(), name: "Claude 5h".into(), weekly: false, utilization: w.used * 100.0 });
        } else if w.id == "weekly_all" || w.id == "seven_day" {
            out.push(CandyWindow { key: "claude.weekly".into(), name: "Claude weekly".into(), weekly: true, utilization: w.used * 100.0 });
        }
    }
    let codex = st.codex.lock().unwrap().clone();
    if !codex.windows.is_empty() {
        ready = true;
    }
    for w in &codex.windows {
        if w.count.is_some() {
            continue;
        }
        highest = highest.max(w.used * 100.0);
        let l = w.label.to_lowercase();
        if l.contains("5h") || l.contains("hour") {
            out.push(CandyWindow { key: format!("codex.{}", w.id), name: "Codex 5h".into(), weekly: false, utilization: w.used * 100.0 });
        } else if l.contains("week") {
            out.push(CandyWindow { key: format!("codex.{}", w.id), name: "Codex weekly".into(), weekly: true, utilization: w.used * 100.0 });
        }
    }
    let ag = st.antigravity.lock().unwrap().clone();
    for w in &ag.windows {
        if w.count.is_some() {
            continue;
        }
        highest = highest.max(w.used * 100.0);
        let l = (w.id.clone() + " " + &w.label).to_lowercase();
        let weekly = l.contains("week");
        let session = ["5h", "5-hour", "hour", "session"].iter().any(|k| l.contains(k));
        if weekly || session {
            out.push(CandyWindow { key: format!("antigravity.{}", w.id), name: format!("Antigravity {}", w.label), weekly, utilization: w.used * 100.0 });
        }
    }
    (out, ready, highest)
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        // Wait for the first token pass so the baseline is real
        std::thread::sleep(std::time::Duration::from_secs(5));
        loop {
            {
                let st = app.state::<crate::AppState>();
                let tokens = st.tokens.lock().unwrap().clone();
                let (windows, ready, highest) = candy_windows(&app);
                let lang = crate::i18n::resolve_lang(&st.cfg.lock().unwrap().lang);
                let mut e = st.companion.engine.lock().unwrap();
                e.lang = lang;
                let today = tokens.today_by_provider();
                let has_data = tokens.updated_at > 0 && !tokens.providers.is_empty();
                e.update(&today, &tokens.today, BurnTier::of(tokens.burn_per_min), highest >= 90.0, has_data);
                e.grant_candies(&windows, ready);
                if let Some(id) = e.state.active.as_ref().map(|a| a.current_id()) {
                    if !e.details.contains_key(&id) {
                        e.ensure_details(id);
                    }
                }
                publish(&app, &mut e);
            }
            for _ in 0..60 {
                if TICK.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    fn line(base: u32, rarity: Rarity, forms: u32) -> EvoLine {
        fn chain(id: u32, left: u32) -> EvoNode {
            EvoNode { id, children: if left == 0 { vec![] } else { vec![chain(id + 1, left - 1)] } }
        }
        let mut names = BTreeMap::new();
        for id in base..base + forms {
            names.insert(id, BTreeMap::from([("en".to_string(), format!("Mon{id}"))]));
        }
        EvoLine::new(base, chain(base, forms - 1), rarity, names)
    }
    impl Provider for Fake {
        fn line(&self, base_id: u32) -> Result<EvoLine, String> {
            Ok(match base_id {
                10 => line(10, Rarity::Common, 3),
                20 => line(20, Rarity::Rare, 1),
                DITTO => line(DITTO, Rarity::Rare, 1),
                _ => line(base_id, Rarity::Common, 2),
            })
        }
        fn base_index(&self) -> Result<Vec<BaseSpecies>, String> {
            Ok(vec![BaseSpecies { id: 10, capture_rate: 255 }, BaseSpecies { id: 20, capture_rate: 45 }])
        }
        fn base_species(&self, _id: u32) -> Result<Option<BaseSpecies>, String> {
            Ok(None)
        }
        fn details(&self, id: u32) -> Result<Details, String> {
            Ok(Details { id, name: "x".into(), height: 1, weight: 1, gender_rate: 4, types: vec![], stats: BTreeMap::from([("hp".to_string(), 45), ("attack".to_string(), 49)]), abilities: vec![crate::pokeapi::AbilityOption { name: "overgrow".into(), slot: 1, is_hidden: false }], moves: vec![crate::pokeapi::MoveOption { name: "tackle".into(), level: 1 }] })
        }
    }

    fn engine(seed: u64) -> Engine {
        let mut s = State::default();
        s.rng = Rng::new(seed);
        Engine::new(s, Box::new(Fake), "en").with_clock(|| 1_000_000)
    }
    fn today(n: u64) -> BTreeMap<String, u64> {
        BTreeMap::from([("claude".to_string(), n)])
    }

    #[test]
    fn thresholds_split_the_graduation_total_over_the_forms() {
        // 3 forms: 1/6, 2/6, 3/6 of 750M
        assert_eq!(phase_threshold(Rarity::Common, 3, 0, 1), 125_000_000);
        assert_eq!(phase_threshold(Rarity::Common, 3, 1, 1), 250_000_000);
        assert_eq!(phase_threshold(Rarity::Common, 3, 2, 1), 375_000_000);
        assert_eq!(phase_threshold(Rarity::Legendary, 1, 0, 1), 6_000_000_000);
        assert_eq!(phase_threshold(Rarity::Common, 1, 0, 2), 375_000_000, "a graduated line grows twice as fast");
        assert_eq!(egg_price(Some(Rarity::Uncommon)), 2_500_000_000);
        assert_eq!(egg_price(Some(Rarity::Rare)), 4_000_000_000);
    }

    #[test]
    fn first_observation_is_a_baseline_and_only_growth_counts() {
        let mut e = engine(1);
        e.update(&today(1_000_000), "2026-09-16", BurnTier::Idle, false, true);
        assert_eq!(e.state.egg_usage, 0, "no retroactive credit");
        e.update(&today(3_000_000), "2026-09-16", BurnTier::Idle, false, true);
        assert_eq!(e.state.egg_usage, 2_000_000);
        // A drop rebases without credit; a new day credits everything reported
        e.update(&today(500_000), "2026-09-16", BurnTier::Idle, false, true);
        assert_eq!(e.state.egg_usage, 2_000_000);
        e.update(&today(1_500_000), "2026-09-17", BurnTier::Idle, false, true);
        assert_eq!(e.state.egg_usage, 3_500_000);
        assert_eq!(e.display, DisplayState::Egg);
    }

    #[test]
    fn egg_hatches_at_five_million_with_overflow_and_the_shiny_roll_is_reproducible() {
        let mut e = engine(1);
        e.update(&today(0), "d", BurnTier::Idle, false, true);
        e.update(&today(EGG_HATCH + 10), "d", BurnTier::Normal, false, true);
        let a = e.state.active.clone().expect("hatched");
        assert_eq!(a.used_at_stage, 10, "overflow carried into the hatchling");
        assert!([10, 20].contains(&a.base_id));
        assert!(NATURES.contains(&a.nature.as_str()));
        assert_eq!(e.display, DisplayState::LevelUp);
        assert!(matches!(e.events[0], Event::Hatched { .. }));
        assert_eq!(a.profile.level, 5);
        assert_eq!(a.profile.ability, "overgrow", "details enrich the profile at hatch");
        // Same seed, same roll
        let mut f = engine(1);
        f.update(&today(0), "d", BurnTier::Idle, false, true);
        f.update(&today(EGG_HATCH + 10), "d", BurnTier::Normal, false, true);
        assert_eq!(f.state.active.unwrap().base_id, a.base_id);
    }

    #[test]
    fn evolution_walks_the_line_and_graduation_fills_the_dex() {
        let mut e = engine(1);
        e.hatch(10);
        let thr0 = e.threshold();
        e.apply_usage(thr0 + 5);
        let a = e.state.active.clone().unwrap();
        assert_eq!((a.stage, a.current_id(), a.used_at_stage), (1, 11, 5));
        assert!(matches!(e.last_event, Some(Event::Evolved { id: 11, .. })));
        e.apply_usage(e.threshold());
        assert_eq!(e.state.active.as_ref().unwrap().current_id(), 12);
        e.apply_usage(e.threshold());
        assert!(e.state.active.is_none(), "graduated");
        assert_eq!(e.state.dex.len(), 1);
        assert_eq!(e.state.dex[0].chain, vec![10, 11, 12]);
        assert!(e.state.collected_finals.contains("10:12"));
        assert_eq!(e.state.dex[0].profile.level, 100);
        // The next hatch of the same line grows twice as fast
        e.hatch(10);
        assert!(e.state.active.as_ref().unwrap().growth_boost);
        assert_eq!(e.threshold(), thr0 / 2);
    }

    #[test]
    fn candy_pays_on_the_rising_edge_and_weekly_pays_five() {
        let mut e = engine(1);
        let w = |u: f64| vec![CandyWindow { key: "claude.session".into(), name: "5h".into(), weekly: false, utilization: u }, CandyWindow { key: "claude.weekly".into(), name: "w".into(), weekly: true, utilization: u }];
        assert!(e.grant_candies(&w(100.0), false).is_empty(), "limits not loaded: nothing yet");
        assert!(e.grant_candies(&w(100.0), true).is_empty(), "already full on first run: seeded, not paid");
        assert!(e.grant_candies(&w(100.0), true).is_empty());
        e.grant_candies(&w(50.0), true);
        let g = e.grant_candies(&w(100.0), true);
        assert_eq!(g.iter().map(|x| x.count).sum::<u32>(), 1 + WEEKLY_CANDIES);
        assert_eq!(e.item_count(Item::RareCandy), 6);
        assert!(e.grant_candies(&w(100.0), true).is_empty(), "no double pay while it stays full");
    }

    #[test]
    fn bag_and_shop_run_on_spent_tokens() {
        let mut e = engine(3);
        e.hatch(10);
        assert!(!e.use_candy(), "none owned");
        e.state.used_since_install = 3_000_000_000;
        assert!(e.can_buy(Item::RareCandy));
        assert!(e.buy(Item::RareCandy));
        assert_eq!(e.available_tokens(), 3_000_000_000 - CANDY_PRICE);
        assert!(e.use_candy());
        assert_eq!(e.state.active.as_ref().unwrap().used_at_stage, CANDY_XP);
        let before = e.state.active.as_ref().unwrap().nature.clone();
        assert!(e.buy(Item::Mint));
        let after = e.use_mint().unwrap();
        assert_ne!(before, after);
        assert!(!e.buy(Item::ShinyCharm), "3B charm is out of reach");
        e.state.used_since_install = 10_000_000_000;
        assert!(e.buy(Item::ShinyCharm));
        assert!(!e.can_buy(Item::ShinyCharm), "held once, not sold twice");
        // A bought egg sends the companion off as released and guarantees the tier
        assert!(e.buy_egg(Some(Rarity::Rare)));
        assert!(e.state.active.is_none());
        assert_eq!(e.state.egg_tier, Some(Rarity::Rare));
        assert!(e.state.dex[0].released_at.is_some());
        assert!(!e.can_buy_egg(None), "no companion to send off");
    }

    #[test]
    fn guaranteed_egg_rejects_a_lower_roll_and_the_index_filter_holds() {
        let mut e = engine(1);
        e.state.egg_tier = Some(Rarity::Rare);
        e.hatch(10); // a common line
        assert!(e.state.active.is_none(), "the guarantee is kept");
        assert!(e.state.egg_tier.is_some());
        let b = e.choose_base().unwrap();
        assert_eq!(b, 20, "only the rare base passes the filter");
    }

    #[test]
    fn difficulty_rescales_progress_instead_of_evolving() {
        let mut e = engine(1);
        e.hatch(10);
        e.apply_usage(100_000_000); // 80 % of the first 125M threshold
        e.set_difficulty(0.5, 1.0);
        let a = e.state.active.as_ref().unwrap();
        assert_eq!(a.stage, 0);
        assert_eq!(a.used_at_stage, 50_000_000);
        assert_eq!(e.threshold(), 62_500_000);
        assert_eq!(e.price(Item::Mint), MINT_PRICE);
        e.set_difficulty(0.5, 2.0);
        assert_eq!(e.price(Item::Mint), MINT_PRICE * 2);
    }

    #[test]
    fn display_state_follows_burn_and_limits() {
        let mut e = engine(1);
        e.hatch(20);
        e.event_until = 0;
        e.update(&today(0), "d", BurnTier::Idle, false, true);
        e.update(&today(10), "d", BurnTier::Normal, false, true);
        assert_eq!(e.display, DisplayState::Working);
        e.update(&today(20), "d", BurnTier::Blazing, false, true);
        assert_eq!(e.display, DisplayState::Focus);
        e.update(&today(20), "d", BurnTier::Idle, true, true);
        assert_eq!(e.display, DisplayState::Tired);
        e.update(&BTreeMap::new(), "d", BurnTier::Idle, false, false);
        assert_eq!(e.display, DisplayState::Sleep);
        assert_eq!(BurnTier::of(50_000.0), BurnTier::Normal);
        assert_eq!(BurnTier::of(500_000.0), BurnTier::Blazing);
    }

    #[test]
    fn view_describes_the_line_and_stats_use_the_nature() {
        let mut e = engine(1);
        e.hatch(10);
        let v = view(&e);
        assert_eq!(v.line, vec![(10, "current".to_string()), (11, "future".to_string()), (12, "future".to_string())]);
        assert!(!v.is_final);
        assert_eq!(v.name, "Mon10");
        let d = e.provider.details(10).unwrap();
        let mut p = Profile::generate(1);
        p.ivs.attack = 31;
        p.level = 50;
        let adamant = computed_stats(&d, &p, "adamant");
        let neutral = computed_stats(&d, &p, "hardy");
        assert!(adamant[1].value > neutral[1].value);
        assert_eq!(neutral[0].value, (2 * 45 + p.ivs.hp) * 50 / 100 + 60);
    }

    #[test]
    fn state_round_trips_through_json() {
        let mut e = engine(1);
        e.hatch(10);
        let t = serde_json::to_string(&e.state).unwrap();
        let back: State = serde_json::from_str(&t).unwrap();
        assert_eq!(back, e.state);
        let old: State = serde_json::from_str("{}").unwrap();
        assert!(old.active.is_none() && old.growth_difficulty == 1.0);
    }
}
