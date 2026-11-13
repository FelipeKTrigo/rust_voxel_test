//! Parte 2 — relógio do mundo: ciclo dia/noite, semanas e fases da lua, posição do sol e da
//! lua, e cometas raros. Tudo puro (sem GPU) para ser testável; `SkyState` é o que vai ao shader.
use glam::Vec3;

/// Duração de um dia de jogo em segundos reais.
pub const DAY_LENGTH_SECS: f64 = 600.0;
pub const WEEK_DAYS: u32 = 7;
/// Ciclo lunar: 4 semanas (28 dias), 8 fases.
pub const LUNAR_CYCLE_DAYS: u32 = 28;
/// O mundo novo começa pouco depois do nascer do sol.
pub const START_TIME_OF_DAY: f32 = 0.30;
/// Desloca a lua no início para que a primeira noite tenha lua crescente visível.
const LUNAR_START_OFFSET_DAYS: f64 = 10.0;
/// Quantas noites (em média) têm um cometa: 1 em ~4.
const COMET_NIGHT_CHANCE: f32 = 0.25;
pub const COMET_DURATION_SECS: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoonPhase {
    New,
    WaxingCrescent,
    FirstQuarter,
    WaxingGibbous,
    Full,
    WaningGibbous,
    LastQuarter,
    WaningCrescent,
}

impl MoonPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "Lua nova",
            Self::WaxingCrescent => "Lua crescente",
            Self::FirstQuarter => "Quarto crescente",
            Self::WaxingGibbous => "Crescente gibosa",
            Self::Full => "Lua cheia",
            Self::WaningGibbous => "Minguante gibosa",
            Self::LastQuarter => "Quarto minguante",
            Self::WaningCrescent => "Lua minguante",
        }
    }

    /// Fase (0..1; 0 = nova, 0.5 = cheia) -> oitavo mais próximo.
    pub fn from_phase(phase: f32) -> Self {
        let i = ((phase.rem_euclid(1.0) * 8.0).round() as usize) % 8;
        [
            Self::New, Self::WaxingCrescent, Self::FirstQuarter, Self::WaxingGibbous,
            Self::Full, Self::WaningGibbous, Self::LastQuarter, Self::WaningCrescent,
        ][i]
    }
}

/// Relógio do mundo (só avança em jogo; persiste no save).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldClock {
    /// Segundos de jogo desde o início do dia 0 (0:00).
    pub elapsed: f64,
}

impl Default for WorldClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldClock {
    pub fn new() -> Self {
        Self { elapsed: START_TIME_OF_DAY as f64 * DAY_LENGTH_SECS }
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt as f64;
    }

    /// Fixa a hora do dia (0 = meia-noite, 0.25 = nascer do sol, 0.5 = meio-dia, 0.75 = pôr do sol).
    pub fn set_time_of_day(&mut self, tod: f32) {
        let day = self.day() as f64;
        self.elapsed = (day + tod.rem_euclid(1.0) as f64) * DAY_LENGTH_SECS;
    }

    /// Dia (0-based).
    pub fn day(&self) -> u32 {
        (self.elapsed / DAY_LENGTH_SECS).floor().max(0.0) as u32
    }

    pub fn time_of_day(&self) -> f32 {
        ((self.elapsed / DAY_LENGTH_SECS).fract() as f32).rem_euclid(1.0)
    }

    /// Semana (0-based).
    pub fn week(&self) -> u32 {
        self.day() / WEEK_DAYS
    }

    /// Dia da semana 0..=6.
    pub fn weekday(&self) -> u32 {
        self.day() % WEEK_DAYS
    }

    /// Fase da lua contínua: 0 = nova, 0.5 = cheia, volta a 1.0 = nova.
    pub fn moon_phase(&self) -> f32 {
        let cycle = LUNAR_CYCLE_DAYS as f64 * DAY_LENGTH_SECS;
        ((self.elapsed + LUNAR_START_OFFSET_DAYS * DAY_LENGTH_SECS) / cycle).fract() as f32
    }

    pub fn moon_phase_kind(&self) -> MoonPhase {
        MoonPhase::from_phase(self.moon_phase())
    }

    /// Luz da lua (0 na nova, 1 na cheia).
    #[allow(dead_code)]
    pub fn moon_illumination(&self) -> f32 {
        0.5 - 0.5 * (self.moon_phase() * std::f32::consts::TAU).cos()
    }

    /// Ângulo do sol no arco: 0 = nascendo, π/2 = a pino, π = poente.
    fn sun_angle(&self) -> f32 {
        (self.time_of_day() - 0.25) * std::f32::consts::TAU
    }

    /// Direção (unitária) para o sol. +X = leste; o sol nasce no leste e se põe no oeste.
    pub fn sun_dir(&self) -> Vec3 {
        let a = self.sun_angle();
        Vec3::new(a.cos(), a.sin(), 0.3).normalize()
    }

    /// A lua fica `fase` de volta atrás do sol: nova = junto do sol, cheia = oposta.
    pub fn moon_dir(&self) -> Vec3 {
        let a = self.sun_angle() - self.moon_phase() * std::f32::consts::TAU;
        Vec3::new(a.cos(), a.sin(), -0.25).normalize()
    }

    /// 0 de noite .. 1 de dia, com crepúsculo suave.
    pub fn daylight(&self) -> f32 {
        smoothstep(-0.10, 0.18, self.sun_dir().y)
    }

    #[allow(dead_code)]
    pub fn is_night(&self) -> bool {
        self.daylight() < 0.05
    }

    /// "Dia 3 · Semana 1 · 14:30"
    pub fn hud_text(&self) -> String {
        let minutes = (self.time_of_day() * 24.0 * 60.0) as u32 % (24 * 60);
        format!(
            "Dia {} · Semana {} ({}/{}) · {:02}:{:02}",
            self.day() + 1,
            self.week() + 1,
            self.weekday() + 1,
            WEEK_DAYS,
            minutes / 60,
            minutes % 60
        )
    }

    /// Cometa ativo agora, se houver.
    pub fn comet(&self, seed: u32) -> Option<Comet> {
        comet_at(seed, self.elapsed)
    }

    pub fn sky_state(&self, seed: u32) -> SkyState {
        SkyState {
            sun_dir: self.sun_dir(),
            daylight: self.daylight(),
            moon_dir: self.moon_dir(),
            moon_phase: self.moon_phase(),
            time_of_day: self.time_of_day(),
            day: self.day(),
            comet: self.comet(seed),
        }
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash_u32(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Cometa em trânsito: segue o arco entre `from` e `to` (direções unitárias no céu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comet {
    pub from: Vec3,
    pub to: Vec3,
    /// 0 = começando, 1 = terminando.
    pub progress: f32,
}

/// Cometa da noite `night` (noite que começa no entardecer do dia `night`), se houver:
/// `(início em segundos de jogo, from, to)`. Determinístico em (seed, noite).
pub fn comet_for_night(seed: u32, night: u32) -> Option<(f64, Vec3, Vec3)> {
    let h = |salt: u32| unit(hash_u32(seed ^ night.wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b)));
    if h(1) > COMET_NIGHT_CHANCE {
        return None;
    }
    // A noite vai de tod 0.75 a 1.25 (do dia seguinte); o cometa aparece no miolo dela.
    let t_night = 0.15 + 0.7 * h(2);
    let start = (night as f64 + 0.75 + 0.5 * t_night as f64) * DAY_LENGTH_SECS;
    let az = h(3) * std::f32::consts::TAU;
    let el0 = 0.55 + 0.6 * h(4);
    let drift = (0.5 + 0.6 * h(5)) * if h(6) > 0.5 { 1.0 } else { -1.0 };
    let el1 = (el0 - 0.35 - 0.25 * h(7)).max(0.15);
    let dir = |az: f32, el: f32| Vec3::new(el.cos() * az.cos(), el.sin(), el.cos() * az.sin());
    Some((start, dir(az, el0), dir(az + drift, el1)))
}

pub fn comet_at(seed: u32, elapsed: f64) -> Option<Comet> {
    let day = (elapsed / DAY_LENGTH_SECS).floor() as i64;
    // A noite que cobre `elapsed` é a do dia `day` (após o poente) ou a do dia anterior (madrugada).
    for night in [day - 1, day] {
        if night < 0 {
            continue;
        }
        if let Some((start, from, to)) = comet_for_night(seed, night as u32) {
            let p = ((elapsed - start) / COMET_DURATION_SECS as f64) as f32;
            if (0.0..1.0).contains(&p) {
                return Some(Comet { from, to, progress: p });
            }
        }
    }
    None
}

/// Tudo que o shader de céu/iluminação precisa.
#[derive(Clone, Copy, Debug)]
pub struct SkyState {
    pub sun_dir: Vec3,
    pub daylight: f32,
    pub moon_dir: Vec3,
    pub moon_phase: f32,
    pub time_of_day: f32,
    pub day: u32,
    pub comet: Option<Comet>,
}

impl SkyState {
    /// Meio-dia fixo (testes e ferramentas).
    #[allow(dead_code)]
    pub fn noon() -> Self {
        let mut c = WorldClock::new();
        c.set_time_of_day(0.5);
        c.sky_state(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, tod: f32) -> WorldClock {
        WorldClock { elapsed: (day as f64 + tod as f64) * DAY_LENGTH_SECS }
    }

    #[test]
    fn new_world_starts_in_the_morning() {
        let c = WorldClock::new();
        assert_eq!(c.day(), 0);
        assert!((c.time_of_day() - START_TIME_OF_DAY).abs() < 1e-4);
        assert!(c.daylight() > 0.5, "manhã clara");
    }

    #[test]
    fn day_wraps_and_counts() {
        let mut c = at(0, 0.99);
        c.advance((0.02 * DAY_LENGTH_SECS) as f32);
        assert_eq!(c.day(), 1);
        assert!((c.time_of_day() - 0.01).abs() < 1e-3);
    }

    #[test]
    fn sun_rises_east_peaks_and_sets_west() {
        let sunrise = at(0, 0.25).sun_dir();
        let noon = at(0, 0.5).sun_dir();
        let sunset = at(0, 0.75).sun_dir();
        let midnight = at(0, 0.0).sun_dir();
        assert!(sunrise.y.abs() < 0.01 && sunrise.x > 0.9, "nasce no leste: {sunrise:?}");
        assert!(noon.y > 0.9);
        assert!(sunset.y.abs() < 0.01 && sunset.x < -0.9, "poente no oeste: {sunset:?}");
        assert!(midnight.y < -0.9);
        for i in 0..100 {
            assert!((at(0, i as f32 / 100.0).sun_dir().length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn daylight_follows_the_sun() {
        assert!(at(0, 0.5).daylight() > 0.99);
        assert!(at(0, 0.0).daylight() < 0.01);
        assert!(at(0, 0.5).daylight() > at(0, 0.27).daylight());
        // Amanhecer e anoitecer são graduais
        let mid = at(0, 0.25).daylight();
        assert!(mid > 0.05 && mid < 0.95, "crepúsculo suave: {mid}");
        assert!(at(0, 0.0).is_night() && !at(0, 0.5).is_night());
    }

    #[test]
    fn weeks_and_weekdays() {
        for day in 0..60u32 {
            let c = at(day, 0.4);
            assert_eq!(c.week(), day / 7);
            assert_eq!(c.weekday(), day % 7);
            assert!(c.weekday() < WEEK_DAYS);
        }
        assert!(at(6, 0.1).hud_text().starts_with("Dia 7 · Semana 1 (7/7)"));
        assert!(at(7, 0.5).hud_text().starts_with("Dia 8 · Semana 2 (1/7)"));
        assert!(at(0, 0.5).hud_text().ends_with("12:00"));
    }

    #[test]
    fn lunar_cycle_is_four_weeks_with_all_phases() {
        let a = at(3, 0.3).moon_phase();
        let b = at(3 + LUNAR_CYCLE_DAYS, 0.3).moon_phase();
        assert!((a - b).abs() < 1e-3, "ciclo de 28 dias");
        assert_eq!(LUNAR_CYCLE_DAYS, 4 * WEEK_DAYS);
        let mut seen = std::collections::HashSet::new();
        for d in 0..LUNAR_CYCLE_DAYS {
            seen.insert(format!("{:?}", at(d, 0.5).moon_phase_kind()));
        }
        assert_eq!(seen.len(), 8, "as 8 fases aparecem: {seen:?}");
        // Iluminação: nova ≈ 0, cheia ≈ 1
        assert!(MoonPhase::from_phase(0.0) == MoonPhase::New && MoonPhase::from_phase(0.5) == MoonPhase::Full);
        let mut min: f32 = 1.0;
        let mut max: f32 = 0.0;
        for d in 0..LUNAR_CYCLE_DAYS {
            let l = at(d, 0.5).moon_illumination();
            min = min.min(l);
            max = max.max(l);
        }
        assert!(min < 0.05 && max > 0.95, "{min} {max}");
    }

    #[test]
    fn first_night_has_a_visible_moon() {
        let c = at(0, 0.9);
        assert!(c.moon_illumination() > 0.3);
    }

    #[test]
    fn moon_is_opposite_the_sun_at_full_moon_and_with_it_at_new() {
        // Procura um dia de lua cheia e um de lua nova.
        let full = (0..LUNAR_CYCLE_DAYS).map(|d| at(d, 0.5))
            .min_by(|a, b| (a.moon_phase() - 0.5).abs().total_cmp(&(b.moon_phase() - 0.5).abs())).unwrap();
        let new = (0..LUNAR_CYCLE_DAYS).map(|d| at(d, 0.5))
            .min_by(|a, b| a.moon_phase().min(1.0 - a.moon_phase()).total_cmp(&b.moon_phase().min(1.0 - b.moon_phase()))).unwrap();
        assert!(full.sun_dir().dot(full.moon_dir()) < -0.6, "cheia: oposta ao sol");
        assert!(new.sun_dir().dot(new.moon_dir()) > 0.6, "nova: junto do sol");
        // Lua cheia está alta à meia-noite.
        let mut f = full;
        f.set_time_of_day(0.0);
        assert!(f.moon_dir().y > 0.5, "lua cheia alta à meia-noite: {:?}", f.moon_dir());
    }

    #[test]
    fn set_time_of_day_keeps_the_day() {
        let mut c = at(5, 0.1);
        c.set_time_of_day(0.75);
        assert_eq!(c.day(), 5);
        assert!((c.time_of_day() - 0.75).abs() < 1e-4);
    }

    #[test]
    fn comets_are_rare_deterministic_and_nocturnal() {
        let seed = 42;
        let nights: Vec<_> = (0..400u32).filter_map(|n| comet_for_night(seed, n).map(|c| (n, c))).collect();
        let frac = nights.len() as f32 / 400.0;
        assert!((0.12..0.40).contains(&frac), "raros: {frac}");
        assert_eq!(comet_for_night(seed, nights[0].0), comet_for_night(seed, nights[0].0));
        assert_ne!(
            (0..50u32).map(|n| comet_for_night(7, n).is_some()).collect::<Vec<_>>(),
            (0..50u32).map(|n| comet_for_night(8, n).is_some()).collect::<Vec<_>>(),
            "depende da seed"
        );
        for (n, (start, from, to)) in &nights {
            let c = WorldClock { elapsed: *start };
            assert!(c.daylight() < 0.05, "cometa só de noite (noite {n}, tod {})", c.time_of_day());
            assert!((from.length() - 1.0).abs() < 1e-4 && (to.length() - 1.0).abs() < 1e-4);
            assert!(from.y > 0.0 && to.y > 0.0, "acima do horizonte");
        }
    }

    #[test]
    fn comet_appears_and_vanishes_quickly() {
        let seed = 42;
        let (n, (start, ..)) = (0..400u32).find_map(|n| comet_for_night(seed, n).map(|c| (n, c))).unwrap();
        let _ = n;
        assert!(comet_at(seed, start - 1.0).is_none());
        let mid = comet_at(seed, start + COMET_DURATION_SECS as f64 * 0.5).unwrap();
        assert!((mid.progress - 0.5).abs() < 1e-3);
        assert!(comet_at(seed, start + COMET_DURATION_SECS as f64 + 0.1).is_none(), "some rápido");
        // Em um dia inteiro sem cometa não aparece nada durante o dia.
        for i in 0..200 {
            let e = (n as f64 + 0.3 + 0.4 * i as f64 / 200.0) * DAY_LENGTH_SECS;
            assert!(comet_at(seed, e).is_none(), "nada de cometa de dia");
        }
    }
}
