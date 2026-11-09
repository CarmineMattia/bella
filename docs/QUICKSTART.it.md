# Prima prova — Term Social 0.1.0-alpha.3

Questa alpha permette di provare chat, gruppi e agente **su questo computer**, aprendo più terminali. La comunicazione tra PC diversi non è ancora disponibile.

## Avvio

Estrai il pacchetto, apri un terminale nella cartella estratta ed esegui:

```bash
./start
```

Al primo avvio scegli il nome, il modello e la cartella di lavoro dell’agente. Tab passa al campo successivo, Ctrl+U lo svuota, Invio salva e apre la chat. Non servono Rust o un compilatore. Servono Python 3.11+, un terminale UTF-8 e, per l’agente, un modello GGUF e `llama-server` già installati.

Il programma riutilizza il modello locale se è già attivo, oppure prova ad avviarlo. Non scarica modelli. Se il modello non è disponibile, la chat si apre comunque. Per saltare l’avvio del modello:

```bash
./start --chat-only
```

Il modello avviato dal programma rimane attivo dopo la chiusura della chat. Quando hai finito:

```bash
./start --stop-model
```

Questo comando ferma solo un modello avviato dal launcher, non un server esterno.

## Prova in due terminali

```bash
./start --name alice
```

In un secondo terminale, dalla stessa cartella:

```bash
./start --name bob
```

Scrivi in `lobby`: i messaggi devono comparire nell’altra finestra. Crea un gruppo con Ctrl+N; nell’altra finestra usa `/join NOME`.

## Prova dell’agente

Apri `/agent` e chiedi una breve risposta. In un gruppo, usa `@bot` seguito dalla richiesta.

Per provare un comando innocuo:

```text
@bot Usa lo strumento shell per eseguire esattamente: printf alpha-ok
```

Controlla il comando proposto prima di premere Y. N o Esc lo rifiutano. **Stop / Ctrl+X** interrompe la risposta e annulla le approvazioni ancora in attesa. Un comando già approvato termina entro il proprio timeout: le sue azioni non vengono annullate.

## Controlli utili

- **F1:** guida completa.
- **F2:** stato del modello; R ripete il controllo.
- **F3 / clic destro:** fissa, archivia o ripristina una chat.
- **F4:** passa tra chat attive e archivio.
- **F5:** attività dell’agente, risultati e interruzioni dopo un crash.
- **F6:** partecipanti e sessioni locali nella stanza.
- **Ctrl+K:** cerca una conversazione. **Ctrl+F:** cerca nei suoi messaggi.
- **Ctrl+End:** torna agli ultimi messaggi.
- **Ctrl+C:** chiude; attende l’arresto di un’attività in corso. Un secondo Ctrl+C forza l’uscita.

Le bozze e le preferenze sono salvate per nome utente. I nomi non sono account autenticati: chi può leggere il database locale può accedere alle conversazioni. La cartella di lavoro non è una sandbox; i comandi approvati usano i tuoi permessi Linux.

## Backup e diagnosi

```bash
./start --backup /percorso/assoluto/nuovo-backup.sqlite
./start --doctor
```

Il backup non sovrascrive file esistenti e include messaggi, bozze, preferenze e attività. Le impostazioni sono nel file `settings.json`, da copiare separatamente. Per ripristinare, chiudi le sessioni originali e apri una copia del backup con un nuovo nome di file; [istruzioni complete](USER_GUIDE.md#backup-and-restore).

Se l’app si interrompe, le attività non vengono rieseguite automaticamente. F5 mostra quelle interrotte; prima di ripetere una richiesta controlla gli effetti di eventuali comandi già approvati.

[Checklist di prova](ALPHA_TESTING.md) · [Limiti noti](KNOWN_ISSUES.md)

## Profili, immagini e nuovi bot

- **F7** oppure `/profile`: modifica nome visualizzato, descrizione e immagine. L'identificativo locale resta fisso e le chat non cambiano.
- Nel campo Picture incolla il percorso di un file PNG/JPEG senza virgolette (massimo 8 MiB, 4096 × 4096). Vuoto conserva l'immagine; `-` la rimuove. Il database conserva una miniatura, inclusa nei backup.
- Tab cambia campo, Ctrl+U cancella il contenuto, **Ctrl+S salva**, Esc annulla.
- **F8** oppure `/bots`: **N** crea un bot, **E** modifica quello selezionato, **Invio** lo aggiunge o rimuove dalla chat corrente.
- Scegli un handle come `planner`, nome, descrizione, istruzioni, modello facoltativo e immagine. Dopo il salvataggio scrivi `@planner aiutami a organizzare il lavoro`.
- Anche il bot originale `@bot` ha un profilo modificabile. Ogni bot usa server e cartella di lavoro configurati; i comandi richiedono sempre la tua approvazione. Si tratta di bot AI locali, senza collegamento a Telegram.

## Due computer (alpha.3)

Sul computer host avvia `python3 server/hub.py --data ./hub-data/hub.sqlite --bind 0.0.0.0`. Su ogni client avvia `./start --network` e inserisci l'indirizzo LAN dell'host. Ctrl+R registra un account usando l'invito nel file `hub-data/hub.invite`; Invio accede a un account esistente. F9 mostra connessione e coda. `./start --network --network-setup` riapre l'accesso; `./start --network-logout` revoca la sessione. Senza `--network` resti nelle chat locali separate.

HTTP trasmette credenziali e messaggi in chiaro: usa una LAN fidata oppure HTTPS. I gruppi sono pubblici per gli account registrati sull'hub. Le menzioni ricevute non avviano automaticamente i tuoi bot. Consulta [la guida di rete](NETWORK.md) per configurazione e limiti.

## Destinatario: Gruppo o Bot

Sopra il campo del messaggio trovi **Group / Bot**. Clicca Bot per parlare con l'agente senza ripetere `@bot`; clicca Group per tornare alla chat normale. **F10** scorre tra gruppo e bot disponibili. Puoi anche usare `/to group`, `/to bot` o `/to @planner`. Una menzione esplicita seleziona quel bot anche per i messaggi successivi. La scelta viene ricordata per chat. Le risposte del bot in un gruppo restano visibili al gruppo; My agent va sempre al bot.
