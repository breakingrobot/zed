# compose-basic — attendu (`03` C1–C3)

- Seul `app` démarre (`runServices`) ; nom de projet dérivé comme le CLI de référence.
- Deux projets ouverts simultanément ne partagent pas leurs fichiers d'override (ADR-003 `BuildDir`).
- Avec la variante `Wslc` : refus explicite « compose non supporté » (ADR-006, ADR-009).
