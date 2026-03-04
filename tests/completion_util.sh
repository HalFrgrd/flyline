# This utility file provides a function and its completion handler to test 
# readline variable emulation in flyline.
#
# Usage: source tests/completion_util.sh
# Then try: flyline_comp_util <tab>

flyline_comp_util() {
    echo "flyline_comp_util called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}



_flyline_comp_util_completions() {
    local cur prev words cword
    _init_completion || return

    # We use previous word to trigger different completion behaviors that 
    # correspond to readline variables.
    
    case "$prev" in
        --filenames)
            compopt -o filenames
            # Use mapfile to read compgen output into an array, preserving spaces in filenames
            mapfile -t COMPREPLY < <(compgen -f -- "$cur")
            return 0
            ;;

        --quoting-desired)
            # compopt -o fullquote # Only available in newer bash versions
            compopt -o filenames # So we use -o filenames to get proper quoting of options with spaces
            COMPREPLY=( "multi word option" )
            return 0
            ;;
            
        --suppress-quote)
            compopt -o noquote
            COMPREPLY=( "multi word option" )
            return 0
            ;;
        
        --dont-suppress-append)
            COMPREPLY=( "foo" )
            return 0
            ;;

        --suppress-append)
            # Setting -o nospace prevents the default space from being appended
            compopt -o nospace
            COMPREPLY=( "foo" )
            return 0
            ;;

    esac

    # Default completion shows available flags
    local opts="--filenames --quoting-desired --suppress-quote --suppress-append"
    COMPREPLY=( $(compgen -W "$opts" -- "$cur") )
}

# Register the completion function
complete -F _flyline_comp_util_completions flyline_comp_util
echo "flyline_comp_util loaded. Try 'flyline_comp_util <tab>'"


flyline_comp_util_default_filenames() {
    echo "flyline_comp_util_default_filenames called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}

_flyline_comp_util_completions_default_filenames() {
    local cur=${COMP_WORDS[COMP_CWORD]}

    mapfile -t COMPREPLY < <(compgen -f -- "$cur")
}


complete -F _flyline_comp_util_completions_default_filenames -o filenames flyline_comp_util_default_filenames
echo "flyline_comp_util_default_filenames loaded. Try 'flyline_comp_util_default_filenames <tab>'"


# TODO add tests for bashdefault and default fallback