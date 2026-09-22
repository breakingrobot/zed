# Plan d'ingénierie — support Dev Containers robuste (Zed)

Dossier de travail local (branche `plan/devcontainers`), **hors de l'arbre mdBook publié** (`docs/book.toml` → `src = "src"`).
Rien ici n'a été poussé ni posté. Base : `main` = `16c9aa7ea6` (2026-09-22).

## État

| Phase | Livrable | État |
|---|---|---|
| 0 — Préparation | `raw/` (données GitHub brutes, `raw/INDEX.md` ; `gh` non authentifié → API REST publique) | ✅ |
| 1 — Architecture actuelle | `01-current-architecture.md` | ✅ |
| 2 — Contributions existantes | `02-contrib-*.md`, `02-overlap-matrix.md` | ✅ |
| 🛑 Point d'arrêt 1 | | en attente de validation |
| 3 — Conformité spec | `03-spec-compliance.md` | — |
| 4 — Topologies (local, WSL, SSH, imbriqué, WSLc) | `04-topologies.md` | — |
| 🛑 Point d'arrêt 2 | | — |
| 5 — Architecture cible + ADRs | `05-target-architecture.md`, `adr/` | — |
| 6 — Pile de PRs + message de coordination (brouillon) | `06-pr-stack.md` | — |
| 7 — Stratégie de test + fixtures | `07-test-strategy.md`, `fixtures/` | — |
| 8 — Revue adverse | `08-adversarial-review.md` | — |

## Sources analysées (branches locales en lecture seule)

| Branche locale | Origine | Rapport |
|---|---|---|
| `review/pr-60975-lifecycle` | PR #60975 (alex-berger) | `02-contrib-pr-60975.md` |
| `review/pr-62680-remote` | PR #62680 (alexdhill) | `02-contrib-pr-62680.md` |
| `review/pupeno-wsl` | pupeno/zed `wsl-devcontainers` | `02-contrib-pupeno-wsl.md` |
| `review/pupeno-remote` | pupeno/zed `remote-devcontainer` | `02-contrib-pupeno-remote.md` |

Les worktrees d'essai `H:\Sources\zed-wt\<nom>` et les branches `trial/*` citées dans les rapports ont été **supprimées**
après analyse (merges d'essai jamais commités) ; les logs de build/test sont conservés dans `raw/build-logs/`.
Les branches `review/*` restent disponibles localement.

## Conventions

- Chaque affirmation est sourcée : `chemin:ligne` (sur `main` sauf mention), SHA, ou fichier de `raw/`.
- Hypothèses : ✅ confirmée · ❌ infirmée · ⚠️ partielle.
- Contraintes amont retenues (`CONTRIBUTING.md`) : fonctionnalités à faire valider en discussion avant PR ;
  PRs « une seule chose » avec tests ; plafond de **3 PRs ouvertes par auteur** ; pas de « refactorings géants » ;
  politique IA (humain qui comprend le code, pas d'agents autonomes, messages aux mainteneurs écrits par un humain).
