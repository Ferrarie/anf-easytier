# Shared loader: source this file to load the repo-root .env.
# Existing environment variables always win.
_ANF_ENV_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.env"
if [ -f "$_ANF_ENV_FILE" ]; then
  while IFS='=' read -r _anf_key _anf_val; do
    case "$_anf_key" in
      ''|\#*) continue ;;
    esac
    if [ -z "${!_anf_key:-}" ]; then
      export "$_anf_key=$_anf_val"
    fi
  done < "$_ANF_ENV_FILE"
fi
unset _ANF_ENV_FILE _anf_key _anf_val
