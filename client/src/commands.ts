import * as path from 'path'
import * as vscode from 'vscode'
import * as lc from 'vscode-languageclient/node'
import { Extension } from './extension'
import { log } from './log'
import { Disposable } from 'vscode-languageclient/node'

export function commandList(extension: Extension): Disposable[] {
  let commandList = []
  commandList.push(vscode.commands.registerCommand('mcshader.restart', restartExtension(extension)))
  commandList.push(vscode.commands.registerCommand('mcshader.virtualMerge', virtualMergedDocument(extension)))
  return commandList
}

type Command = (...args: any[]) => unknown

function restartExtension(e: Extension): Command {
  return async () => {
    vscode.window.showInformationMessage('Reloading Minecraft shaders language server...')
    await e.deactivate()
    await e.activate(e.context).catch(log.error)
  }
}

function virtualMergedDocument(e: Extension): Command {
  const getVirtualDocument = async (path: string): Promise<string | null> => {
    let content: string = ''
    log.info(path)
    try {
      content = await e.client.sendRequest<string>(lc.ExecuteCommandRequest.type.method, {
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
      let extName = path.extname(uri.path)
      return getVirtualDocument(uri.path.replace('.flattened' + extName, extName))
    }
  }

  e.context.subscriptions.push(vscode.workspace.registerTextDocumentContentProvider('mcshader', docProvider))

  return async () => {
    if (vscode.window.activeTextEditor.document.languageId != 'glsl') return

    const extIndex = vscode.window.activeTextEditor.document.uri.path.lastIndexOf('.')
    const uri = vscode.window.activeTextEditor.document.uri.path
      .substring(0, extIndex)
      + '.flattened'
      + vscode.window.activeTextEditor.document.uri.path
        .slice(extIndex)
    const path = vscode.Uri.parse(`mcshader:${uri}`)

    const doc = await vscode.workspace.openTextDocument(path)
    docProvider.onDidChangeEmitter.fire(path)
    await vscode.window.showTextDocument(doc, {
      viewColumn: vscode.ViewColumn.Two,
      preview: true
    })
  }
}
