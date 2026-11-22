# Omnyra — Distribución (paso 7c)

Cómo se publica una versión de Omnyra para Windows, Debian/Ubuntu y Arch, con actualizaciones automáticas firmadas.

## Qué genera cada versión

| Sistema | Archivo | Se actualiza solo |
|---|---|---|
| Windows | `Omnyra_<v>_x64-setup.exe` (NSIS, por usuario, español/inglés) | Sí |
| Windows | `Omnyra_<v>_x64_es-ES.msi` / `en-US.msi` (WiX) | No (reinstalar) |
| Debian/Ubuntu | `Omnyra_<v>_amd64.deb` (depende de `ffmpeg` y `libayatana-appindicator3-1`, recomienda `gnome-keyring`) | No (paquete del sistema) |
| Linux (cualquiera) | `Omnyra_<v>_amd64.AppImage` | Sí |
| Arch | `packaging/arch/PKGBUILD` (AUR) | No (lo actualiza pacman/yay) |

Más `latest.json` (manifiesto del actualizador) y `SHA256SUMS`.

## 1. Una sola vez

### Claves de firma del actualizador

```bash
npm run tauri signer generate -- -w ~/.tauri/omnyra.key
```

Guarda la contraseña. **No subas nunca la clave privada.** En GitHub → *Settings → Secrets and variables → Actions*, crea:

| Secreto | Valor |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contenido de `~/.tauri/omnyra.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | la contraseña |
| `OMNYRA_UPDATER_PUBKEY` | contenido de `~/.tauri/omnyra.key.pub` |
| `OMNYRA_GOOGLE_CLIENT_ID`, `OMNYRA_GOOGLE_CLIENT_SECRET`, `OMNYRA_TWITCH_CLIENT_ID`, `OMNYRA_BROKER_URL`, `OMNYRA_INSTAGRAM_CLIENT_ID`, `OMNYRA_TIKTOK_CLIENT_KEY` | los mismos valores que tu `.env` |

Si pierdes la clave privada, las instalaciones existentes ya no podrán actualizarse solas (habrá que reinstalar una vez con la clave nueva).

### Cómo funciona el actualizador

- Solo las compilaciones de la CI de releases llevan la clave pública y la URL (`OMNYRA_UPDATER_PUBKEY`, `OMNYRA_UPDATER_ENDPOINT`, incrustadas por `src-tauri/build.rs`). Las compilaciones locales no se actualizan solas y lo dicen en Ajustes.
- La comprobación la hace Rust (`check_update` / `install_update`): descarga `https://github.com/<owner>/omnyra/releases/latest/download/latest.json`, verifica la firma minisign del instalador y lo instala. Nunca se instala nada sin firma válida.

## 2. Publicar una versión

1. Sube la versión en `package.json`, `Cargo.toml` (`[workspace.package]`) y `src-tauri/tauri.conf.json` (las tres iguales) y añade su sección a `CHANGELOG.md` (`## [x.y.z] - fecha`).
2. Commit y etiqueta: `git tag v1.0.0 && git push origin main v1.0.0`.
3. El workflow `.github/workflows/release.yml` comprueba que la etiqueta coincide con la versión, compila y firma en Ubuntu 22.04 y Windows, genera `latest.json` (`scripts/release/latest-json.mjs`) y crea la release con las notas del CHANGELOG.

## 3. AUR

1. En `packaging/arch/PKGBUILD`, cambia `_owner=OWNER` por tu usuario de GitHub (una vez).
2. Tras publicar la release: `cd packaging/arch && updpkgsums && makepkg --printsrcinfo > .SRCINFO`.
3. Prueba: `makepkg -si` en una carpeta limpia.
4. Primer envío: `git clone ssh://aur@aur.archlinux.org/omnyra.git`, copia `PKGBUILD` y `.SRCINFO`, commit y `git push`. (Necesitas cuenta en AUR con tu clave SSH.)

## 4. Si GitHub sigue suspendido (R1)

Todo el trabajo pesado está en `scripts/ci/*.sh` y `scripts/release/latest-json.mjs`; portar a Codeberg/Forgejo Actions o GitLab CI es copiar los pasos. Cambia también `OMNYRA_UPDATER_ENDPOINT` y la `url` del `PKGBUILD` al nuevo alojamiento.

## 5. Compilar a mano

```bash
scripts/ci/bundle.sh deb,appimage        # Linux → ./artifacts
scripts/ci/bundle.sh msi,nsis            # Windows (Git Bash)
```

Para firmar en local: exporta `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` y `OMNYRA_UPDATER_ARTIFACTS=1`.
