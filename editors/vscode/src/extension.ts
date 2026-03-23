import * as vscode from 'vscode';
import * as path from 'path';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('quantalang');
    const serverPath = config.get<string>('serverPath', 'quantac');

    const serverOptions: ServerOptions = {
        command: serverPath,
        args: ['lsp'],
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'quantalang' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.quanta'),
        },
    };

    client = new LanguageClient(
        'quantalang',
        'QuantaLang Language Server',
        serverOptions,
        clientOptions
    );

    client.start().catch((err) => {
        // The language server isn't available yet — syntax highlighting
        // still works without it. Log silently so users aren't alarmed.
        console.log(
            'QuantaLang language server not found. ' +
            'Syntax highlighting is active. ' +
            'Install quantac and set quantalang.serverPath for full LSP support.'
        );
    });
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
