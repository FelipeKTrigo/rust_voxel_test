//! Fase 9 — Level of Detail por distância ao jogador.
use crate::ui::settings::LodLevel;

/// Resolução da malha de um chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Lod {
    /// Greedy mesh completo.
    #[default]
    Full,
    /// Greedy mesh simplificado (minérios viram pedra, sem detalhes nem faces inferiores).
    Merged,
    /// Apenas o topo das colunas + saias laterais.
    Light,
}

/// Distâncias (em chunks, Chebyshev) até onde cada nível vale: `(full_até, merged_até)`.
/// `None` = LOD desligado (tudo em resolução total).
pub fn thresholds(level: LodLevel) -> Option<(i32, i32)> {
    match level {
        LodLevel::Off => None,
        LodLevel::Low => Some((3, 6)),
        LodLevel::Medium => Some((6, 12)),
        // O plano original: 0–8 completo, 8–16 fundido, 16+ ultraleve.
        LodLevel::High => Some((8, 16)),
    }
}

pub fn lod_for_distance(dist_chunks: i32, level: LodLevel) -> Lod {
    match thresholds(level) {
        None => Lod::Full,
        Some((full, merged)) => {
            if dist_chunks <= full {
                Lod::Full
            } else if dist_chunks <= merged {
                Lod::Merged
            } else {
                Lod::Light
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_is_always_full() {
        for d in 0..40 {
            assert_eq!(lod_for_distance(d, LodLevel::Off), Lod::Full);
        }
    }

    #[test]
    fn high_matches_the_plan() {
        assert_eq!(lod_for_distance(0, LodLevel::High), Lod::Full);
        assert_eq!(lod_for_distance(8, LodLevel::High), Lod::Full);
        assert_eq!(lod_for_distance(9, LodLevel::High), Lod::Merged);
        assert_eq!(lod_for_distance(16, LodLevel::High), Lod::Merged);
        assert_eq!(lod_for_distance(17, LodLevel::High), Lod::Light);
    }

    #[test]
    fn monotonic_in_distance_and_level() {
        for level in [LodLevel::Low, LodLevel::Medium, LodLevel::High] {
            let mut prev = Lod::Full;
            for d in 0..40 {
                let l = lod_for_distance(d, level);
                assert!(l >= prev, "LOD não pode melhorar com a distância");
                prev = l;
            }
        }
        // Mais qualidade nunca deixa um chunk mais grosseiro.
        for d in 0..40 {
            assert!(lod_for_distance(d, LodLevel::High) <= lod_for_distance(d, LodLevel::Medium));
            assert!(lod_for_distance(d, LodLevel::Medium) <= lod_for_distance(d, LodLevel::Low));
        }
    }
}
