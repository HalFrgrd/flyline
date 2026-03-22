# Copilot
flyline agent-mode \
    --system-prompt "Be concise. Answer with a JSON array of <=3 items with objects containing command and description. Command will be a bash command." \
    --command copilot --reasoning-effort low --prompt 

# Claude
# Claude has a --system-prompt flag so we could use that instead of making flyline prepend its system prompt, but for consistency with other agents we'll just prepend the system prompt in flyline.
flyline agent-mode \
    --system-prompt "Be concise. Answer with a JSON array of <=3 items with objects containing command and description. Command will be a bash command." \
    --command claude --effort low --prompt 

# Others:
# Feel free to add more agent examples!
