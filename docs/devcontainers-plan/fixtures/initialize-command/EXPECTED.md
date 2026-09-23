# initialize-command — attendu (`03` L2, ADR-007, ADR-010)

- Exécuté sur l'**hôte moteur**, cwd = dossier du projet sur cet hôte, **à chaque ouverture** (conteneur existant compris).
- Hôte POSIX : `["/bin/sh","-c","echo init > .devcontainer/initialized.txt"]` ; hôte Windows : `cmd /c …`.
- Variante d'échec : remplacer par `"exit 3"` ⇒ l'ouverture échoue avec un message.
Sur `main` : sauté si le conteneur existe ; exit ≠ 0 ignoré ; `/bin/sh` introuvable sous Windows.
