# blinky

Playing around w/ the USB Hid interface for my status indicator light (an Embrava Blynclight, `2c0d:0010`).

## Usage

```
blinky hold --color red                          # steady, stays on
blinky hold --color red --duration 2000          # steady for 2s, then off
blinky hold --color red --flash-speed fast       # flashing, stays on
blinky zap --color orange --num-blinks 20        # strobe, then off
blinky off
```

Colors are CSS color names. `cargo install --path .` installs `blinky` and `blinky-askpass`.

## Blink when a YubiKey wants a touch

`blinky-askpass` is an `SSH_ASKPASS` program. When ssh waits for a touch on a FIDO key it posts a macOS notification saying what asked (the git operation and repo, the directory, and the program that started it) and flashes the light purple. When the touch arrives or ssh gives up, the light goes off and the notification is removed. Passphrase and host key prompts are passed through to the terminal.

Notifications need `terminal-notifier` (`brew install terminal-notifier`). Without a light plugged in it still notifies.

To use it for git over ssh:

```
git config --global core.sshCommand "env SSH_ASKPASS=$HOME/.cargo/bin/blinky-askpass SSH_ASKPASS_REQUIRE=force ssh"
```

`SSH_ASKPASS_REQUIRE=force` makes ssh call the hook even when it has a terminal. Export the same two variables in your shell to cover plain `ssh` as well.
