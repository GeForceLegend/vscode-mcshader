import path = require('path')
import * as vscode from 'vscode'
import * as lsp from 'vscode-languageclient/node'
import { Extension } from './extension'
import { log } from './log'

export type Command = (...args: any[]) => unknown

export function restartExtension(e: Extension): Command {
  return async () => {
    vscode.window.showInformationMessage('Reloading Minecraft shaders language server...')
    await e.deactivate()
    await e.activate(e.context).catch(log.error)
  }
}

export function virtualMergedDocument(e: Extension): Command {
  const getVirtualDocument = async (path: string): Promise<string | null> => {
    let content: string = ''
    log.info(path)
    try {
      content = await e.lspClient.sendRequest<string>(lsp.ExecuteCommandRequest.type.method, {
        command: 'virtualMerge',
        arguments: [path]
      })
    } catch (e) { }

    return content
  }

  const docProvider = new class implements vscode.TextDocumentContentProvider {
    onDidChangeEmitter = new vscode.EventEmitter<vscode.Uri>()
    onDidChange = this.onDidChangeEmitter.event

    provideTextDocumentContent(uri: vscode.Uri, __: vscode.CancellationToken): vscode.ProviderResult<string> {
      return getVirtualDocument(uri.path.replace('.flattened' + path.extname(uri.path), path.extname(uri.path)))
    }
  }

  e.context.subscriptions.push(vscode.workspace.registerTextDocumentContentProvider('mcshader', docProvider))

  return async () => {
    if (vscode.window.activeTextEditor.document.languageId != 'glsl') return

    const uri = vscode.window.activeTextEditor.document.uri.path
      .substring(0, vscode.window.activeTextEditor.document.uri.path.lastIndexOf('.'))
      + '.flattened.'
      + vscode.window.activeTextEditor.document.uri.path
        .slice(vscode.window.activeTextEditor.document.uri.path.lastIndexOf('.') + 1)
    const path = vscode.Uri.parse(`mcshader:${uri}`)

    const doc = await vscode.workspace.openTextDocument(path)
    docProvider.onDidChangeEmitter.fire(path)
    await vscode.window.showTextDocument(doc, {
      viewColumn: vscode.ViewColumn.Two,
      preview: true
    })
  }
}
