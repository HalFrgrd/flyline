
source ./bash-preexec/bash-preexec.sh

eval "$(starship init bash)"



if [ -f "target/debug/jobu" ]; then
	eval "$(target/debug/jobu activate)"
else
	echo "Error: target/debug/jobu is not a file"
fi

