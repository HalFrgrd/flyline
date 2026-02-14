# Flyline
A modern `readline` alternative for bash.

Features:
- Undo and redo support
- Cursor animations
- Fuzzy history suggestions
- Fuzzy autocompletions
- Integration with bash autocomplete
- Mouse support:
    - Click to move cursor in buffer
    - Hover over command for tooltips

# Installation
Download the latest `libflyline.so`.
In your `.bashrc` (or in your current bash session): `enable -f /path/to/libflyline.so flyline`.


# Integrations
## VsCode:
- Shell integration WIP
- I'd recommend setting `terminal.integrated.minimumContrastRatio = 1` to prevent the cell's foreground colour changing it's under the cursor.
