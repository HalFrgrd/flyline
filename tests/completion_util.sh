# This utility file provides a function and its completion handler to test 
# readline variable emulation in flyline.
#
# Usage: source tests/completion_util.sh
# Then try: fl_comp_util <tab>

fl_comp_util() {
    echo "fl_comp_util called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}


_fl_comp_util_completions() {
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
        
        --nosort)
            compopt -o nosort
            COMPREPLY=( "banana" "apple" "cherry" )
            return 0
            ;;

        --fallback-to-default)
            # Don't provide any completions, which should trigger the default completion behavior
            COMPREPLY=()
            return 0
            ;;
        
        --fallback-to-default-filenames)
            compopt -o filenames
            COMPREPLY=()
            return 0
            ;;

        --env-var-test)
            compopt -o filenames
            COMPREPLY=('$HOME/foo/$baz.txt')
            return 0
            ;;

    esac

    # Default completion shows available flags
    local opts="--filenames --quoting-desired --suppress-quote --dont-suppress-append --suppress-append --nosort --fallback-to-default --fallback-to-default-filenames --env-var-test"
    COMPREPLY=( $(compgen -W "$opts" -- "$cur") )
}

# Register the completion function
complete -F _fl_comp_util_completions fl_comp_util
echo "fl_comp_util loaded. Try 'fl_comp_util <tab>'"


fl_comp_util_default_filenames() {
    echo "fl_comp_util_default_filenames called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}


complete -F _fl_comp_util_completions -o filenames fl_comp_util_default_filenames
echo "fl_comp_util_default_filenames loaded. Try 'fl_comp_util_default_filenames <tab>'"



fl_comp_util_bashdefault() {
    echo "fl_comp_util_bashdefault called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}

complete -F _fl_comp_util_completions -o bashdefault fl_comp_util_bashdefault
echo "fl_comp_util_bashdefault loaded. Try 'fl_comp_util_bashdefault <tab>'"



fl_comp_util_dirnames() {
    echo "fl_comp_util_dirnames called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}

complete -F _fl_comp_util_completions -o dirnames fl_comp_util_dirnames
echo "fl_comp_util_dirnames loaded. Try 'fl_comp_util_dirnames <tab>'"



fl_comp_util_plusdirs() {
    echo "fl_comp_util_plusdirs called with args:"
    for arg in "$@"; do
        echo "  '$arg'"
    done
}

complete -F _fl_comp_util_completions -o plusdirs fl_comp_util_plusdirs
echo "fl_comp_util_plusdirs loaded. Try 'fl_comp_util_plusdirs <tab>'"


alias fl_comp_alias='fl_comp_util --nosort'
echo "fl_comp_alias loaded. Try 'fl_comp_alias <tab>'"

