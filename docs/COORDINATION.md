# Coordinamento agenti

Su questo progetto lavorano due agenti: **Remo** e **ChatGPT**. Carmine decide priorità, commit, push e merge.

Regole:
1. Leggi questo file prima di modificare qualsiasi cosa.
2. Prima di iniziare, aggiungi una riga in "In corso": data e ora (Europe/Rome), nome, cosa fai, file che tocchi.
3. Se un file è segnato da un altro agente come "in corso", non toccarlo: scrivi in "Richieste" e avvisa Carmine.
4. Quando finisci, sposta la riga in "Fatto", con i file cambiati e i test eseguiti (con l'esito reale).
5. Niente commit, push o remote senza un OK esplicito di Carmine. Non leggere né modificare .env o file di credenziali.

## In corso
- 2026-11-09 12:05 CET · ChatGPT (Codex) · Launcher browser affidabile, esiti strutturati e dettagli separati dai messaggi · file: src/browser.rs, src/agent.rs, src/main.rs, src/ui.rs, src/tests.rs, docs/BROWSER_ACTIONS.md (nuovo), docs/USER_GUIDE.md, CHANGELOG.md. Nessun commit; nessun accesso a credenziali.

## Richieste

## Fatto
- 2026-11-09 12:05 · Remo · applicata patch "prestito bot solo chat" (opt-in per bot, nessun tool per ospiti, quota 5/ora, requester nei task) · file come sopra · test: ./scripts/check test --offline 55 ok, 3 ignorati. Backup pre-patch: /home/cr1m3/work/backups/term-social-before-delegate-20261109-1203.tgz. Nessun commit.
