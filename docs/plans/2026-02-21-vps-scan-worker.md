# VPS Scan Worker — Spec

## Problème

Le web scanner actuel (`POST /api/scan`) utilise l'API GitHub REST pour récupérer les commits. Limitations :

1. **Rate limit** : 5 000 req/hr avec un token. Un gros repo (100k commits) = ~1 000 requêtes = 20% du budget horaire en un seul scan
2. **Analyse incomplète** : le Worker JS fait du scoring simplifié (pas de deps, tests, security, languages — juste les commits)
3. **Pas de filtre temporel** : on fetch tous les commits même si on veut juste 2025+
4. **Scaling impossible** : cron job sur 500 repos = rate limit atteint en minutes

## Solution

Déporter le scan sur le VPS OVH (`ubuntu@vps-139a77b3.vps.ovh.net`) via un worker HTTP qui :
- Clone les repos avec `git clone --bare --shallow-since`
- Analyse avec le binary Rust (identique au CLI local)
- Upload les résultats vers l'API Cloudflare existante

`git clone` utilise le protocole git, **pas** l'API REST GitHub → pas soumis au rate limit 5 000/hr.

## Architecture

```
Frontend (Astro)
    │
    │ POST /api/scan {repo: "user/repo", since: "2025-01-01"}
    ▼
Cloudflare Worker (proxy)
    │
    │ POST /scan {repo, since}
    ▼
VPS Worker (Axum HTTP)           ← nouveau
    │
    ├─ git clone --bare --shallow-since=2025-01-01 https://github.com/{repo}.git /tmp/{uuid}
    ├─ vibereport /tmp/{uuid} --json --since 2025-01-01
    ├─ POST https://api.vibereport.dev/api/reports  (store results)
    ├─ rm -rf /tmp/{uuid}
    │
    └─ return {id, ai_ratio, score, grade, roast, url, ...}
```

Le Worker Cloudflare devient un simple proxy : il forward la requête au VPS et relay la réponse. Fallback sur l'analyse GitHub API actuelle si le VPS est down.

## VPS Worker — Composants

### 1. HTTP Server (Axum)

Un seul endpoint :

```
POST /scan
Content-Type: application/json

{
  "repo": "user/repo",         // requis
  "since": "2025-01-01",       // optionnel, default: 2025-01-01
  "auth_token": "secret"       // shared secret pour auth Worker↔VPS
}
```

Réponse : le JSON output de `vibereport --json`.

### 2. Clone strategy

```bash
git clone --bare --shallow-since="2025-01-01" \
  https://github.com/{user}/{repo}.git \
  /tmp/vibereport-{uuid}
```

- `--bare` : pas de working tree, que les objets git → rapide, léger
- `--shallow-since` : que les commits depuis la date → réduit la bande passante
- `/tmp/{uuid}` : isolation par scan, cleanup après

Timeout : 60s pour le clone (repos géants). Kill + cleanup si dépassé.

### 3. Analyse

```bash
vibereport /tmp/vibereport-{uuid} --json --since 2025-01-01
```

Le binary doit être compilé et déployé sur le VPS. Output JSON parsé par le worker.

Note : `--bare` clone = pas de working tree → l'analyse projet (deps, tests, languages) ne fonctionne pas. Deux options :
- **Option A** : clone non-bare (`git clone --depth 1` séparé pour les fichiers projet) — plus lourd
- **Option B** : accepter que le web scan ne fait que l'analyse git (commits/AI ratio) et pas l'analyse projet — cohérent avec le comportement actuel du Worker JS

Recommandation : **Option B** pour le MVP, Option A plus tard si besoin.

### 4. Concurrence

- Queue en mémoire (tokio bounded channel, capacité 20)
- Max 5 clones simultanés (semaphore)
- Requêtes au-delà → 429 Too Many Requests
- Un scan ~= 5-15s (clone + analyse + cleanup)

### 5. Sécurité

- Auth par shared secret (header `Authorization: Bearer {token}`)
- Le VPS n'expose que le port du worker (ex: 3001), derrière un reverse proxy nginx avec HTTPS
- Rate limit nginx : 30 req/min par IP source
- Pas de repos privés (que les publics GitHub pour l'instant)

## Changements requis

### CLI Rust — flag `--since`

Ajouter à `Cli` :

```rust
/// Only analyze commits since this date (YYYY-MM-DD, "6m", "1y", or "all")
#[arg(long, default_value = "all")]
since: String,
```

Parser dans `analyze_repo` : passer un `Option<DateTime<Utc>>` cutoff. Filtrer les commits dans la boucle de walk sans toucher au root hash (fingerprint inchangé).

### API Cloudflare — proxy vers VPS

Dans `POST /api/scan` : tenter d'abord le VPS, fallback sur le code GitHub API actuel si timeout/erreur.

```typescript
// Pseudo-code
const vpsResult = await fetchWithTimeout(VPS_URL + '/scan', { repo, since }, 30_000);
if (vpsResult.ok) return vpsResult.json();
// fallback: existing GitHub API logic
```

Ajouter var d'env : `VPS_SCAN_URL`, `VPS_AUTH_TOKEN`.

### DB — champs période

Ajouter à `reports` :

```sql
ALTER TABLE reports ADD COLUMN period_start TEXT;  -- ISO date, NULL = full history
ALTER TABLE reports ADD COLUMN period_end TEXT;
ALTER TABLE reports ADD COLUMN scan_source TEXT DEFAULT 'cli';  -- 'cli' | 'web_api' | 'web_vps'
```

### Fingerprint

- **CLI local** : inchangé (`root_commit_sha:remote_url`), le walk complet retrouve toujours le root
- **VPS (shallow clone)** : utiliser `remote_url` seul comme fingerprint (le root d'un shallow clone n'est pas le vrai root)

Pour éviter les conflits fingerprint entre CLI et VPS sur le même repo, normaliser : si le fingerprint contient `github.com`, extraire l'URL et l'utiliser comme clé unique. Les deux sources upsertent sur la même entrée.

## Déploiement VPS

```bash
# Sur le VPS
# 1. Compiler le binary
cargo build --release --target x86_64-unknown-linux-gnu
scp target/release/vibereport ubuntu@vps-139a77b3.vps.ovh.net:~/vibereport/

# 2. Déployer le worker (nouveau crate ou binary séparé)
scp vps-worker ubuntu@vps-139a77b3.vps.ovh.net:~/vibereport/

# 3. Systemd service
# /etc/systemd/system/vibereport-worker.service
[Unit]
Description=Vibereport VPS Scan Worker
After=network.target

[Service]
User=vibereport
ExecStart=/home/vibereport/vps-worker
Environment=PORT=3001
Environment=AUTH_TOKEN=xxx
Environment=VIBEREPORT_BIN=/home/vibereport/vibereport
Restart=always

[Install]
WantedBy=multi-user.target
```

Nginx reverse proxy :

```nginx
server {
    listen 443 ssl;
    server_name scan.vibereport.dev;

    location / {
        proxy_pass http://127.0.0.1:3001;
        proxy_read_timeout 60s;
        limit_req zone=scan burst=10;
    }
}
```

## Cron jobs (futur)

Avec le VPS worker en place, on peut ajouter un cron qui rescan les repos du leaderboard :

```bash
# Chaque dimanche, rescan les top 100 repos
0 3 * * 0 curl -X POST https://scan.vibereport.dev/rescan-top \
  -H "Authorization: Bearer $TOKEN"
```

Le endpoint `/rescan-top` query la DB pour les 100 repos les plus populaires et queue un scan pour chacun. À 5-15s/scan, ~25 min pour 100 repos.

## Résumé des gains

| Métrique | Avant (GitHub API) | Après (VPS clone) |
|----------|-------------------|-------------------|
| Rate limit | 5 000 req/hr | Quasi illimité |
| Scoring | JS simplifié | Rust complet |
| Filter temporel | Tous les commits | `--shallow-since` natif |
| Coût par scan | ~10-1000 API calls | 1 git clone |
| Cron 100 repos | Impossible | ~25 min |
| Repos privés | Besoin token scope repo | Même (token git) |
