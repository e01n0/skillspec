# Editor support

Syntax highlighting for `.agent` files.

## VS Code

The `vscode/` directory is a complete extension. Install it locally:

```sh
cd editors/vscode
npx @vscode/vsce package        # produces skillspec-0.1.0.vsix
code --install-extension skillspec-0.1.0.vsix
```

Or for development, symlink it into your extensions folder:

```sh
ln -s "$(pwd)/editors/vscode" ~/.vscode/extensions/e01n0.skillspec-0.1.0
```

## Other editors

The TextMate grammar at `vscode/syntaxes/skillspec.tmLanguage.json` works in
any editor that consumes TextMate grammars (Sublime Text, JetBrains IDEs via
the TextMate Bundles plugin, Zed, bat, etc.). Point your editor's grammar
loader at that file with scope `source.skillspec` and file extension `.agent`.

A tree-sitter grammar (which would also unlock highlighting on github.com and
form the parsing backbone for the planned LSP) is on the roadmap.
