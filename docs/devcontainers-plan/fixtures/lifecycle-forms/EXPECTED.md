# lifecycle-forms — attendu (spec, `03-spec-compliance.md` L1, L4, L6, L7)

| Hook | argv exécuté dans le conteneur (`docker exec … <argv>`) |
|---|---|
| onCreate (chaîne) | `["/bin/sh","-c","mkdir -p /tmp/zed-fixture && echo created > /tmp/zed-fixture/on-create"]` |
| updateContent (tableau) | `["/bin/sh","-c","echo 'two words' > /tmp/zed-fixture/update-content"]` — **aucun** shell ajouté |
| postCreate (objet) | `first` : `["/bin/sh","-c","echo a > …"]` ; `second` : `["touch","/tmp/zed-fixture/post create b"]` (espace conservé), en parallèle |
| postStart (chaîne) | idem forme chaîne, une ligne ajoutée par démarrage |
| postAttach (tableau) | `$HOME` développé **dans** le conteneur |

Vérifications : `/tmp/zed-fixture/post create b` existe (1 fichier, pas 2) ; `update-content` contient `two words` ;
marqueurs `~/.devcontainer/.onCreateCommandMarker`, `.updateContentCommandMarker`, `.postCreateCommandMarker`, `.postStartCommandMarker` présents.
Sur `main` : échoue (bug #62964, `docker.rs:377-386`).
