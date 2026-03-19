# Base image for VHS demo GIFs

# Use the published Charmbracelet VHS image by digest
FROM ghcr.io/charmbracelet/vhs@sha256:cbcdcd255e61dd9d278ad25183ae3662c849ab9b104434ac1ba330d27b2883cc AS vhs-base

# Create a non-root user for demos
RUN useradd -m -s /bin/bash john

WORKDIR /app

# Copy the Flyline shared library into the container
COPY --from=flyline-extracted-library /libflyline.so .

# Give john ownership of the app directory
RUN chown -R john:john /app

RUN apt-get update && apt-get install -y --no-install-recommends bash-completion faketime && rm -rf /var/lib/apt/lists/*

USER john

RUN touch /home/john/.bashrc && \
    printf '%s\n' \
    'source /usr/share/bash-completion/bash_completion' \
    'source /etc/bash_completion' \
    'alias ll="ls -alF"' \
    'export HISTTIMEFORMAT="%F %T  "' \
    'export HISTCONTROL=ignoreboth' \
    'export PS1="\e[01;32m\u@\h\e[00m:\e[01;34m~\e[00m\$ "' \
    'export RPS1=""' \
    'enable -f /app/libflyline.so flyline' \
    'flyline --disable-auto-closing-char' \
    >> /home/john/.bashrc

RUN touch /home/john/.bash_history && \
    printf '%s\n' \
 '#1771881194' \
 'ls -la' \
 '#1771881202' \
 'cd projects' \
 '#1771881210' \
 'git status' \
 '#1771881218' \
 'git add .' \
 '#1771881226' \
 'git commit -m "initial commit"' \
 '#1771881234' \
 'cargo build' \
 '#1771881242' \
 'cargo test' \
 '#1771881250' \
 'vim src/main.rs' \
 '#1771881258' \
 'grep -R "TODO" .' \
 '#1771881266' \
 'rg "fn main"' \
 '#1771881274' \
 'cd ..' \
 '#1771881282' \
 'mkdir tmp' \
 '#1771881290' \
 'rm -rf tmp' \
 '#1771881298' \
 'docker ps' \
 '#1771881306' \
 'docker build -t myapp .' \
 '#1771881314' \
 'docker run -it myapp' \
 '#1771881322' \
 'ps aux | grep bash' \
 '#1771881330' \
 'kill -9 12345' \
 '#1771881338' \
 'history | tail' \
 '#1771881346' \
 'echo $PATH' \
 '#1771881354' \
 'export RUST_LOG=debug' \
 '#1771881362' \
 'make clean' \
 '#1771881370' \
 'make' \
 '#1771881378' \
 './target/debug/myapp' \
 '#1771881386' \
 'curl http://localhost:8080' \
 '#1771881394' \
 'wget https://example.com/file.txt' \
 '#1771881402' \
 'tar -xzvf archive.tar.gz' \
 '#1771881410' \
 'ssh user@server' \
 '#1771881418' \
 'scp file.txt user@server:/tmp' \
 '#1771881426' \
 'htop' \
 '#1771881434' \
 'df -h' \
 '#1771881442' \
 'du -sh *' \
 '#1771881450' \
 'alias ll='\''ls -lah'\''' \
 '#1771881458' \
 'source ~/.bashrc' \
 '#1771881466' \
 'printf "Hello\nWorld\n"' \
 '#1771881474' \
 'xargs -0 -I{} echo {}' \
 '#1771881482' \
 'find . -type f -name "*.rs"' \
 '#1771881490' \
 'tree -L 2' \
 '#1771881498' \
 'git checkout -b feature-x' \
 '#1771881506' \
 'git push origin feature-x' \
 '#1771881514' \
 'git pull --rebase' \
 '#1771881522' \
 'cat /etc/os-release' \
 '#1771881530' \
 'uname -a' \
 '#1771881538' \
 'sudo apt update' \
 '#1771881546' \
 'sudo apt upgrade' \
 '#1771881554' \
 'crontab -l' \
 '#1771881562' \
 'crontab -e' \
 '#1771881570' \
 'env | sort' \
 '#1771881578' \
 'set -o vi' \
 '#1771881586' \
 'bind -P' \
 '#1771881594' \
 'clear' \
    >> /home/john/.bash_history


COPY tapes/demo_settings.tape .
COPY tapes/demo_setup.tape .
