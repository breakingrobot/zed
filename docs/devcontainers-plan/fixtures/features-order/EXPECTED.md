# features-order — attendu (`03` F1, L5)

- `b` est ajoutée par `dependsOn` de `c` même si absente de devcontainer.json.
- Ordre d'installation (`/tmp/zed-fixture-install-order` dans l'image) : `a`, `b`, `c`.
- `postCreateCommand` fusionnés (`/tmp/zed-fixture/feature-hooks`) : `a`, `b`, `c`, puis `user`.
⚠️ Non vérifié : la prise en charge de références **locales** (`./features/b`) dans `dependsOn`/`installsAfter` par le CLI de
référence. Avant d'en faire un test Zed, exécuter `devcontainer build` (CLI de référence) sur cette fixture ; sinon remplacer
par des features OCI publiques (`ghcr.io/devcontainers/features/common-utils` + une feature qui déclare `installsAfter`).

Sur `main` : `b` absente (dependsOn non lu), ordre alphabétique, hooks de features non exécutés (`features.rs:280-286`).
