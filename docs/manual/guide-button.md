# The Guide button

The Guide button (the PlayStation, Xbox or Home button) is special, because Steam listens for it. By default padwight treats it as a modifier, the **Guide layer**. A plain press does nothing, so Steam doesn't take over. Hold Guide and press another button for a shortcut:

- **Guide, double tap**: passes Guide on to Steam, which opens the Steam overlay.
- **X**: on-screen keyboard.
- **Y**: on-screen numpad.
- **LB**: media controls.
- **RB**: take a screenshot.
- **L3**: start or stop a screen recording.
- **RT / LT**: left click / right click.
- **Right stick**: moves the mouse.
- **Hold B for 2 seconds**: force quit the focused window.
- **D-pad Right, Down, Left**: Enter, Tab and Escape.
- **D-pad Up**: next profile.

Anything else pressed with Guide held does nothing, so it never reaches the game underneath. You can change these shortcuts for each setup, on its **Layers** tab.

## Force quit

Holding B for 2 seconds closes the program in the focused window. It asks the program to close, and if it doesn't, stops it after 3 seconds. Letting go early cancels it. The desktop and padwight itself are never closed.

## Screenshots and recordings

Screenshots are saved to `Pictures/Screenshots`, in a folder for the game. Recordings are made with gpu-screen-recorder and saved to `Videos/Recordings`, in a folder for the game. If gpu-screen-recorder isn't installed, padwight shows a message saying so.
