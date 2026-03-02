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

flyline_comp_util_default_filenames() {
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
            # Demonstrates rl_filename_completion_desired
            # Setting this tells readline (and flyline) to treat matches as filenames
            # which usually implies quoting them if they contain special chars.
            compopt -o filenames
            # no default:    dirs with slash and no space, files with space. doesnt work with files with spaces in name (considers them separate values)
            # defualt files: dirs with slash and no space, files with space. doesnt work with files with spaces in name (considers them separate values)

            COMPREPLY=( $(compgen -f -- "$cur") )
            return 0
            ;;
            
        --quoting-desired)

            # flyline_comp_util --quoting-desired <TAB> -> file\ with\ spaces.txt
            # flyline_comp_util_default_filenames --quoting-desired <TAB> -> file\ with\ spaces.txt

            # Demonstrates rl_filename_quoting_desired
            # While bash doesn't have a direct flag for this separate from filenames,
            # using filenames usually enables it. We simulate a scenario where
            # we return filenames that need quoting.
            compopt -o filenames # commenting this line results in no backslashed spaces.
            local files="file with spaces.txt"
            # We don't use compgen -f here to force manual handling if needed,
            # but rely on 'filenames' option to trigger quoting
            COMPREPLY=( "$files" )
            return 0
            ;;
            
        --suppress-quote)
            # Demonstrates rl_completion_suppress_quote
            # Setting -o noquote prevents a closing quote from being appended
            compopt -o noquote
            COMPREPLY=( "value without closing quote" )
            return 0
            ;;
            
        --suppress-append)
            # Demonstrates rl_completion_suppress_append
            # Setting -o nospace prevents the default space from being appended
            compopt -o nospace
            COMPREPLY=( "value_without_space" )
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
complete -F _flyline_comp_util_completions -o filenames flyline_comp_util_default_filenames
echo "flyline_comp_util_default_filenames loaded. Try 'flyline_comp_util_default_filenames <tab>'"
