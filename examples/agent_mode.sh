# Copilot
flyline agent-mode \
    --system-prompt "Be concise. Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command." \
    --command copilot --reasoning-effort low --prompt 

# Claude has a --system-prompt flag so we could use that instead of making flyline prepend its system prompt, but for consistency with other agents we'll just prepend the system prompt in flyline.
flyline agent-mode \
    --system-prompt "Be concise. Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command." \
    --command claude --effort low --prompt 

# Codex:
flyline agent-mode \
    --system-prompt "Be concise. Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command." \
    --command codex -a never exec -m 'GPT-5.1-Codex-Mini' --skip-git-repo-check --ephemeral --color always 

# Feel free to add more agent examples!


## Using trigger prefixes

# When you type `: how do find files older than 3 days?`, 
# flyline sees that the buffer starts with the trigger prefix `: ` and sends `how do find files older than 3 days?` (without the prefix)
# to the agent command configured for that trigger prefix.
flyline agent-mode \
    --system-prompt "Be concise..." \
    --trigger-prefix ": " \
    --command copilot --reasoning-effort low --prompt
