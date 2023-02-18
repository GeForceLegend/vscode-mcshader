import * as vscode from 'vscode'
import * as lsp from 'vscode-languageclient/node'
import * as commands from './commands'
import { log } from './log'
import { LanguageClient } from './lspClient'
import * as path from 'path'

export class Extension {
  private statusBarItem: vscode.StatusBarItem | null = null
  private extensionContext: vscode.ExtensionContext | null = null
  private client: lsp.LanguageClient

  readonly extensionID = 'GeForceLegend.vscode-mcshader'

  readonly package: {
    version: string
  } = vscode.extensions.getExtension(this.extensionID)!.packageJSON

  public get context(): vscode.ExtensionContext {
    return this.extensionContext
  }

  public get lspClient(): lsp.LanguageClient {
    return this.client
  }

  public activate = async (context: vscode.ExtensionContext) => {
    this.extensionContext = context

    this.registerCommand('restart', commands.restartExtension)
    this.registerCommand('virtualMerge', commands.virtualMergedDocument)

    log.info('starting language server...')

    const lspBinary = process.env['MCSHADER_DEBUG'] ?
      this.context.asAbsolutePath(path.join('server', 'target', 'debug', 'vscode-mcshader.exe')) :
      this.context.asAbsolutePath(path.join('server', 'vscode-mcshader.exe'))

    this.client = await new LanguageClient(this, lspBinary).startServer()

    log.info('language server started!')
  }

  registerCommand = (name: string, f: (e: Extension) => commands.Command) => {
    const cmd = f(this)
    this.context.subscriptions.push(vscode.commands.registerCommand('mcshader.' + name, cmd))
  }

  deactivate = async () => {
    await this.lspClient.stop()
    while (this.context.subscriptions.length > 0) {
      this.context.subscriptions.pop()?.dispose()
    }
  }

  public updateStatus = (icon: string, text: string) => {
    this.statusBarItem?.dispose()
    this.statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left)
    this.statusBarItem.text = icon + ' [mc-shader] ' + text
    this.statusBarItem.show()
    this.context.subscriptions.push(this.statusBarItem)
  }

  public clearStatus = () => {
    this.statusBarItem?.dispose()
  }
}

export const activate = async (context: vscode.ExtensionContext) => {
  try {
    new Extension().activate(context)
  } catch (e) {
    log.error(`failed to activate extension: ${e}`)
    throw(e)
  }
}