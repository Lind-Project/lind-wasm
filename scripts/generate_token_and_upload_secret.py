import time
import jwt
import requests
from google.cloud import secretmanager
import os


# === CONFIG ===
GITHUB_APP_ID = "1201575" 
GITHUB_ORG = "Lind-Project"
PROJECT_ID = "1049119266483"
PRIVATE_KEY_SECRET = "github-app-private-key"
TARGET_SECRET = "github-bot-token"

# ✅ Trigger Cloud Build to recognize the secret is used
os.environ.get("GITHUB_APP_PRIVATE_KEY")  # This satisfies secretEnv usage

# === LOAD PRIVATE KEY FROM SECRET MANAGER ===
client = secretmanager.SecretManagerServiceClient()
private_key_response = client.access_secret_version(
    request={"name": f"projects/{PROJECT_ID}/secrets/{PRIVATE_KEY_SECRET}/versions/latest"}
)
private_key = private_key_response.payload.data.decode("utf-8")

# === CREATE JWT ===
now = int(time.time())
payload = {
    "iat": now - 60,
    "exp": now + (10 * 60),
    "iss": GITHUB_APP_ID
}
jwt_token = jwt.encode(payload, private_key, algorithm="RS256")

# === LOOK UP INSTALLATION ID ===
headers = {
    "Authorization": f"Bearer {jwt_token}",
    "Accept": "application/vnd.github+json"
}
installations_res = requests.get("https://api.github.com/app/installations", headers=headers)
installations_res.raise_for_status()

installations = installations_res.json()

# Look for the installation tied to your org
installation_id = None
for install in installations:
    if install["account"]["login"].lower() == GITHUB_ORG.lower():
        installation_id = install["id"]
        break

if not installation_id:
    raise Exception(f"No installation found for org {GITHUB_ORG}")

# === EXCHANGE JWT FOR INSTALLATION TOKEN ===
token_res = requests.post(
    f"https://api.github.com/app/installations/{installation_id}/access_tokens",
    headers=headers
)
token_res.raise_for_status()
access_token = token_res.json()["token"]

# === UPLOAD TO SECRET MANAGER ===
parent = f"projects/{PROJECT_ID}/secrets/{TARGET_SECRET}"
client.add_secret_version(
    request={
        "parent": parent,
        "payload": {"data": access_token.encode("utf-8")}
    }
)

print(f"✅ Token for installation {installation_id} stored in {TARGET_SECRET}")
