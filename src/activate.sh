echo 'hello from jobu'
PROMPT_COMMAND='echo $LINENO > $JOBU_FIFO_PATH 2>/dev/null || true'