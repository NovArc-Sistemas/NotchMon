//! PokéAPI, read at runtime and cached on disk, as PokeTokenBar does it: species (capture rate,
//! legendary flag, names), evolution chains, the base-species index used to roll a hatch, battle
//! metadata for the profile page, and the Gen V sprites. Nothing Pokémon-related ships in the binary.
//!
//! Everything lives under `%APPDATA%\codenotch\pokeapi` (JSON, 30 days) and `sprites` (forever).
//! The engine talks to the `Provider` trait so its tests run against a fake without a network.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const API: &str = "https://pokeapi.co/api/v2";
const GRAPHQL: &str = "https://graphql.pokeapi.co/v1beta2";
const SPRITES: &str = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites";
/// Gen V animated sprites exist for #1..=649 only; the hatch pool stops there
pub const MAX_SPECIES: u32 = 649;
pub const DITTO: u32 = 132;
const CACHE_DAYS: u64 = 30;
const TIMEOUT_SECS: u64 = 15;
/// Move learnsets are read from this version group (the last Gen V one)
const VERSION_GROUP: &str = "black-2-white-2";
/// Name languages kept, in PokéAPI's codes
const LANGS: [&str; 8] = ["en", "pt-br", "pt", "es", "fr", "de", "ja", "ko"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    /// capture_rate ceiling per tier; legendaries are judged by their flag, never by the rate
    pub fn ceiling(self) -> Option<u32> {
        match self {
            Rarity::Rare => Some(45),
            Rarity::Uncommon => Some(120),
            Rarity::Common => Some(255),
            Rarity::Legendary => None,
        }
    }
    /// Whether a species with this capture rate is at least this tier (the guaranteed-egg gate)
    pub fn includes(self, capture_rate: u32) -> bool {
        match self.ceiling() {
            Some(c) => capture_rate <= c,
            None => false,
        }
    }
    pub fn from_species(capture_rate: u32, is_legendary: bool, is_mythical: bool) -> Rarity {
        if is_legendary || is_mythical {
            Rarity::Legendary
        } else if capture_rate <= 45 {
            Rarity::Rare
        } else if capture_rate <= 120 {
            Rarity::Uncommon
        } else {
            Rarity::Common
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Rarity::Common => "common",
            Rarity::Uncommon => "uncommon",
            Rarity::Rare => "rare",
            Rarity::Legendary => "legendary",
        }
    }
}

/// A node of an evolution tree; branches (Eevee) are several children
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvoNode {
    pub id: u32,
    pub children: Vec<EvoNode>,
}

impl EvoNode {
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
    }
    pub fn find(&self, id: u32) -> Option<&EvoNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
    pub fn final_ids(&self) -> Vec<u32> {
        if self.children.is_empty() {
            vec![self.id]
        } else {
            self.children.iter().flat_map(|c| c.final_ids()).collect()
        }
    }
    pub fn all_ids(&self) -> Vec<u32> {
        let mut v = vec![self.id];
        for c in &self.children {
            v.extend(c.all_ids());
        }
        v
    }
    /// Only species with Gen V sprites; an unsupported species takes its subtree with it
    pub fn keeping_sprites(&self) -> Option<EvoNode> {
        if self.id == 0 || self.id > MAX_SPECIES {
            return None;
        }
        Some(EvoNode { id: self.id, children: self.children.iter().filter_map(|c| c.keeping_sprites()).collect() })
    }
}

/// What a hatch fixes: the tree, its rarity and every species' names
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvoLine {
    pub base_id: u32,
    pub tree: EvoNode,
    pub rarity: Rarity,
    /// species id → language code → name
    pub names: BTreeMap<u32, BTreeMap<String, String>>,
}

impl EvoLine {
    pub fn new(base_id: u32, tree: EvoNode, rarity: Rarity, names: BTreeMap<u32, BTreeMap<String, String>>) -> Self {
        let tree = tree.keeping_sprites().unwrap_or(EvoNode { id: base_id, children: vec![] });
        EvoLine { base_id, tree, rarity, names }
    }
    pub fn total_forms(&self) -> usize {
        self.tree.depth()
    }
    /// "pt" → pt-br, pt, en; anything else → that language, en
    pub fn name(&self, id: u32, lang: &str) -> String {
        let n = self.names.get(&id);
        let order: &[&str] = if lang == "pt" { &["pt-br", "pt", "en"] } else { &[lang, "en"] };
        for l in order {
            if let Some(s) = n.and_then(|m| m.get(*l)) {
                return s.clone();
            }
        }
        format!("#{id}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BaseSpecies {
    pub id: u32,
    pub capture_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbilityOption {
    pub name: String,
    pub slot: u32,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoveOption {
    pub name: String,
    /// level learned at, level-up moves in the preferred version group only
    pub level: u32,
}

/// Battle metadata for one species (the profile page)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Details {
    pub id: u32,
    pub name: String,
    /// decimetres, hectograms
    pub height: u32,
    pub weight: u32,
    /// female eighths, -1 = genderless
    pub gender_rate: i32,
    pub types: Vec<String>,
    pub stats: BTreeMap<String, u32>,
    pub abilities: Vec<AbilityOption>,
    pub moves: Vec<MoveOption>,
}

impl Details {
    /// Level-up moves known at `level`, earliest first, one per name
    pub fn moves_through(&self, level: u32) -> Vec<MoveOption> {
        let mut best: BTreeMap<&str, u32> = BTreeMap::new();
        for m in &self.moves {
            if m.level <= level {
                let e = best.entry(m.name.as_str()).or_insert(m.level);
                *e = (*e).min(m.level);
            }
        }
        let mut v: Vec<MoveOption> = best.into_iter().map(|(n, l)| MoveOption { name: n.to_string(), level: l }).collect();
        v.sort_by(|a, b| a.level.cmp(&b.level).then(a.name.cmp(&b.name)));
        v
    }
}

/// What the engine needs from PokéAPI. Errors are strings: the engine keeps the egg and retries.
pub trait Provider: Send + Sync {
    fn line(&self, base_id: u32) -> Result<EvoLine, String>;
    fn base_index(&self) -> Result<Vec<BaseSpecies>, String>;
    /// None = not a base species (evolves from something)
    fn base_species(&self, id: u32) -> Result<Option<BaseSpecies>, String>;
    fn details(&self, id: u32) -> Result<Details, String>;
}

// ---------------- the live client ----------------

pub struct Client {
    dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Stamped<T> {
    at: u64,
    v: T,
}

#[derive(Deserialize)]
struct SpeciesDto {
    capture_rate: u32,
    is_legendary: bool,
    is_mythical: bool,
    names: Vec<NameDto>,
    evolution_chain: UrlRef,
    evolves_from_species: Option<serde_json::Value>,
    gender_rate: Option<i32>,
}
#[derive(Deserialize)]
struct NameDto {
    name: String,
    language: NamedRef,
}
#[derive(Deserialize)]
struct NamedRef {
    name: String,
    #[serde(default)]
    url: Option<String>,
}
#[derive(Deserialize)]
struct UrlRef {
    url: String,
}
#[derive(Deserialize)]
struct ChainDto {
    chain: ChainLink,
}
#[derive(Deserialize)]
struct ChainLink {
    species: NamedRef,
    evolves_to: Vec<ChainLink>,
}
#[derive(Deserialize)]
struct PokemonDto {
    name: String,
    height: u32,
    weight: u32,
    types: Vec<TypeDto>,
    abilities: Vec<AbilityDto>,
    stats: Vec<StatDto>,
    moves: Vec<MoveDto>,
}
#[derive(Deserialize)]
struct TypeDto {
    slot: u32,
    #[serde(rename = "type")]
    kind: NamedRef,
}
#[derive(Deserialize)]
struct AbilityDto {
    is_hidden: bool,
    slot: u32,
    ability: NamedRef,
}
#[derive(Deserialize)]
struct StatDto {
    base_stat: u32,
    stat: NamedRef,
}
#[derive(Deserialize)]
struct MoveDto {
    #[serde(rename = "move")]
    mv: NamedRef,
    version_group_details: Vec<MoveVersionDto>,
}
#[derive(Deserialize)]
struct MoveVersionDto {
    level_learned_at: u32,
    move_learn_method: NamedRef,
    version_group: NamedRef,
}

/// The species cache entry: everything the app ever needs from `pokemon-species/{id}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Species {
    pub id: u32,
    pub capture_rate: u32,
    pub is_legendary: bool,
    pub is_mythical: bool,
    pub is_base: bool,
    pub gender_rate: i32,
    pub chain_url: String,
    pub names: BTreeMap<String, String>,
}

fn now() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

fn id_from_url(u: &str) -> u32 {
    u.split('/').filter(|s| !s.is_empty()).last().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(TIMEOUT_SECS)).user_agent("NotchMon/1 (+https://github.com/NovArc-Sistemas/NotchMon)").build()
}

impl Client {
    pub fn new() -> Self {
        let dir = crate::config::config_path().with_file_name("pokeapi");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.with_file_name("sprites"));
        Client { dir }
    }

    pub fn sprites_dir(&self) -> PathBuf {
        self.dir.with_file_name("sprites")
    }

    fn cached<T: serde::de::DeserializeOwned>(&self, name: &str, allow_stale: bool) -> Option<T> {
        let s: Stamped<T> = serde_json::from_str(&std::fs::read_to_string(self.dir.join(name)).ok()?).ok()?;
        if allow_stale || now().saturating_sub(s.at) < CACHE_DAYS * 86_400 {
            Some(s.v)
        } else {
            None
        }
    }

    fn store<T: Serialize>(&self, name: &str, v: &T) {
        if let Ok(t) = serde_json::to_string(&Stamped { at: now(), v }) {
            let _ = std::fs::write(self.dir.join(name), t);
        }
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, String> {
        // Only PokéAPI: the chain URL comes from a response and must not be able to point elsewhere
        if !url.starts_with("https://pokeapi.co/") {
            return Err(format!("refused url {url}"));
        }
        agent().get(url).call().map_err(|e| e.to_string())?.into_json::<T>().map_err(|e| e.to_string())
    }

    /// Fresh from the cache, else fetched; a stale cache entry stands in when the network fails
    fn cached_or_fetch<T: serde::de::DeserializeOwned + Serialize>(&self, name: &str, fetch: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        if let Some(v) = self.cached::<T>(name, false) {
            return Ok(v);
        }
        match fetch() {
            Ok(v) => {
                self.store(name, &v);
                Ok(v)
            }
            Err(e) => self.cached::<T>(name, true).ok_or(e),
        }
    }

    pub fn species(&self, id: u32) -> Result<Species, String> {
        self.cached_or_fetch(&format!("species-{id}.json"), || {
            let d: SpeciesDto = self.get_json(&format!("{API}/pokemon-species/{id}/"))?;
            let mut names = BTreeMap::new();
            for n in d.names {
                if LANGS.contains(&n.language.name.as_str()) {
                    names.insert(n.language.name, n.name);
                }
            }
            Ok(Species {
                id,
                capture_rate: d.capture_rate,
                is_legendary: d.is_legendary,
                is_mythical: d.is_mythical,
                is_base: d.evolves_from_species.map(|v| v.is_null()).unwrap_or(true),
                gender_rate: d.gender_rate.unwrap_or(-1),
                chain_url: d.evolution_chain.url,
                names,
            })
        })
    }

    fn chain(&self, url: &str) -> Result<EvoNode, String> {
        let id = id_from_url(url);
        self.cached_or_fetch(&format!("chain-{id}.json"), || {
            let d: ChainDto = self.get_json(url)?;
            fn node(l: &ChainLink) -> EvoNode {
                EvoNode { id: id_from_url(l.species.url.as_deref().unwrap_or("")), children: l.evolves_to.iter().map(node).collect() }
            }
            Ok(node(&d.chain))
        })
    }

    fn graphql_base_index(&self) -> Result<Vec<BaseSpecies>, String> {
        #[derive(Deserialize)]
        struct Resp {
            data: Data,
        }
        #[derive(Deserialize)]
        struct Data {
            pokemonspecies: Vec<BaseSpecies>,
        }
        let q = format!("{{ pokemonspecies(where: {{evolves_from_species_id: {{_is_null: true}}, id: {{_lte: {MAX_SPECIES}, _neq: {DITTO}}}}}, order_by: {{id: asc}}) {{ id capture_rate }} }}");
        let r: Resp = agent().post(GRAPHQL).send_json(serde_json::json!({ "query": q })).map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
        if r.data.pokemonspecies.is_empty() {
            return Err("empty base index".into());
        }
        Ok(r.data.pokemonspecies)
    }

    /// Downloads a sprite once; returns its file. `animated` = the Gen V GIF, else the static PNG
    pub fn sprite(&self, id: u32, animated: bool, shiny: bool) -> Option<PathBuf> {
        let file = self.sprites_dir().join(format!("{id}{}{}.{}", if shiny { "-sh" } else { "" }, if animated { "-a" } else { "" }, if animated { "gif" } else { "png" }));
        if file.is_file() {
            return Some(file);
        }
        let url = match (animated, shiny) {
            (true, false) => format!("{SPRITES}/pokemon/versions/generation-v/black-white/animated/{id}.gif"),
            (true, true) => format!("{SPRITES}/pokemon/versions/generation-v/black-white/animated/shiny/{id}.gif"),
            (false, false) => format!("{SPRITES}/pokemon/versions/generation-v/black-white/{id}.png"),
            (false, true) => format!("{SPRITES}/pokemon/versions/generation-v/black-white/shiny/{id}.png"),
        };
        self.download(&url, &file)
    }

    /// The egg (`pokemon/egg.png`) and item sprites
    pub fn asset(&self, name: &str) -> Option<PathBuf> {
        let file = self.sprites_dir().join(format!("{name}.png"));
        if file.is_file() {
            return Some(file);
        }
        let url = match name {
            "egg" => format!("{SPRITES}/pokemon/egg.png"),
            other => format!("{SPRITES}/items/{other}.png"),
        };
        self.download(&url, &file)
    }

    fn download(&self, url: &str, file: &PathBuf) -> Option<PathBuf> {
        let resp = agent().get(url).call().ok()?;
        let mut bytes = Vec::new();
        resp.into_reader().take(4 << 20).read_to_end(&mut bytes).ok()?;
        if bytes.is_empty() {
            return None;
        }
        std::fs::write(file, &bytes).ok()?;
        Some(file.clone())
    }
}

use std::io::Read;

/// Standard base64, for the data URLs the pages get sprites as (no crate for 20 lines)
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

impl Provider for Client {
    fn line(&self, base_id: u32) -> Result<EvoLine, String> {
        if let Some(l) = self.cached::<EvoLine>(&format!("line-{base_id}.json"), true) {
            return Ok(l); // a line never changes; the stale rule is not applied
        }
        let sp = self.species(base_id)?;
        let tree = self.chain(&sp.chain_url)?;
        let rarity = Rarity::from_species(sp.capture_rate, sp.is_legendary, sp.is_mythical);
        let mut names = BTreeMap::new();
        for id in tree.keeping_sprites().map(|t| t.all_ids()).unwrap_or_default() {
            let s = if id == base_id { sp.clone() } else { self.species(id)? };
            names.insert(id, s.names);
        }
        let line = EvoLine::new(base_id, tree, rarity, names);
        self.store(&format!("line-{base_id}.json"), &line);
        Ok(line)
    }

    fn base_index(&self) -> Result<Vec<BaseSpecies>, String> {
        self.cached_or_fetch("base-index.json", || self.graphql_base_index())
    }

    fn base_species(&self, id: u32) -> Result<Option<BaseSpecies>, String> {
        let s = self.species(id)?;
        Ok(if s.is_base { Some(BaseSpecies { id, capture_rate: s.capture_rate }) } else { None })
    }

    fn details(&self, id: u32) -> Result<Details, String> {
        self.cached_or_fetch(&format!("pokemon-{id}.json"), || {
            let d: PokemonDto = self.get_json(&format!("{API}/pokemon/{id}/"))?;
            let sp = self.species(id)?;
            let mut types: Vec<(u32, String)> = d.types.into_iter().map(|t| (t.slot, t.kind.name)).collect();
            types.sort();
            let mut abilities: Vec<AbilityOption> = d.abilities.into_iter().map(|a| AbilityOption { name: a.ability.name, slot: a.slot, is_hidden: a.is_hidden }).collect();
            abilities.sort_by_key(|a| a.slot);
            let mut moves = Vec::new();
            for m in d.moves {
                for v in m.version_group_details {
                    if v.version_group.name == VERSION_GROUP && v.move_learn_method.name == "level-up" {
                        moves.push(MoveOption { name: m.mv.name.clone(), level: v.level_learned_at });
                    }
                }
            }
            moves.sort_by(|a, b| a.level.cmp(&b.level).then(a.name.cmp(&b.name)));
            Ok(Details {
                id,
                name: d.name,
                height: d.height,
                weight: d.weight,
                gender_rate: sp.gender_rate,
                types: types.into_iter().map(|t| t.1).collect(),
                stats: d.stats.into_iter().map(|s| (s.stat.name, s.base_stat)).collect(),
                abilities,
                moves,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn rarity_follows_capture_rate_and_the_legendary_flag() {
        assert_eq!(Rarity::from_species(255, false, false), Rarity::Common);
        assert_eq!(Rarity::from_species(120, false, false), Rarity::Uncommon);
        assert_eq!(Rarity::from_species(45, false, false), Rarity::Rare);
        assert_eq!(Rarity::from_species(3, true, false), Rarity::Legendary);
        assert!(Rarity::Rare.includes(45) && !Rarity::Rare.includes(46));
        assert!(!Rarity::Legendary.includes(3), "a legendary guarantee cannot be expressed by rate");
    }

    #[test]
    fn evolution_tree_is_pruned_to_gen_v_and_names_fall_back() {
        let tree = EvoNode { id: 1, children: vec![EvoNode { id: 2, children: vec![EvoNode { id: 3, children: vec![] }, EvoNode { id: 700, children: vec![] }] }] };
        let mut names = BTreeMap::new();
        names.insert(1, BTreeMap::from([("en".to_string(), "Bulbasaur".to_string())]));
        let line = EvoLine::new(1, tree, Rarity::Rare, names);
        assert_eq!(line.tree.find(2).unwrap().children.len(), 1);
        assert_eq!(line.total_forms(), 3);
        assert_eq!(line.tree.final_ids(), vec![3]);
        assert_eq!(line.name(1, "pt"), "Bulbasaur");
        assert_eq!(line.name(3, "en"), "#3");
    }

    #[test]
    fn moves_through_level_keeps_the_earliest_level_per_move() {
        let d = Details {
            id: 1,
            name: "x".into(),
            height: 1,
            weight: 1,
            gender_rate: 4,
            types: vec![],
            stats: BTreeMap::new(),
            abilities: vec![],
            moves: vec![MoveOption { name: "tackle".into(), level: 1 }, MoveOption { name: "tackle".into(), level: 5 }, MoveOption { name: "growl".into(), level: 3 }, MoveOption { name: "razor-leaf".into(), level: 40 }],
        };
        let m = d.moves_through(10);
        assert_eq!(m.iter().map(|x| (x.name.as_str(), x.level)).collect::<Vec<_>>(), vec![("tackle", 1), ("growl", 3)]);
    }
}
