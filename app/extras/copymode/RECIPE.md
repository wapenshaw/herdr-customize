# Copy mode

Navigate and copy terminal text with the keyboard in a small, floating status bar.

![Copy mode](preview.png)

## What you see

A 580 by 36 pixel borderless floating bar uses the current theme's popover,
foreground, accent and muted colors. It shows COPY MODE, the current selection
mode and a short key guide. An unavailable version or missing focused terminal
is explained in the bar instead.

## How it behaves

Open it from a focused terminal with Ctrl-Shift-M, or Cmd-Shift-M on macOS.
The bar takes keyboard focus. h/j/k/l and the arrow keys move the selection's
end; 0 and $ move to line ends. g and G scroll to the top and bottom. Ctrl-U
and Ctrl-D scroll by half a page. v starts a cell selection and Shift-V a line
selection; y copies and closes, while Esc or q clears and closes. Line mode
keeps all rows visited, even when reversing direction. Before v, movement starts
at the terminal cursor; v uses the current moving end if one exists. Agent
statuses do not change the bar.

## Built from

A focusable mod opened by `mod.open:copymode` captures `focused().terminal`,
then opens a borderless window overlay of itself. It uses `commands()` to
check for selection and copy commands, `terminals()` to read selection
endpoints, `run()` for terminal actions, and `closeOverlay()` to close.
`nextCommand()` also accepts `key:<name>` for the same key handling while the
mod is shown, for example `copymode.key:j`. It runs no programs and uses no art.

## Watch out

Selection commands need Herdr 0.1.16 or newer; an older version displays a
message and any key closes the bar. The default global shortcuts can conflict
with existing bindings, so merge them without replacing an occupied key.
Scrolling to the top or bottom changes the view, not the selection endpoint.
Herdr 0.1.16's copy command has no terminal argument, so copying focuses the
original terminal first. Terminal output and reflow can change cell positions.
The window can be dragged and closes on a layout switch.

## Done when

- Opening the shortcut from a terminal shows the bar and takes keyboard input.
- Movement, cell and line selection, copying and cancellation work on that
  terminal without affecting another terminal.
- An unsupported app version explains the requirement and closes on any key.
- `herdr-app check app/extras/copymode` passes without opening an overlay.
