const cp = require('child_process');
const fs = require('fs');
const path = require('path');
const yaml = require('js-yaml');
const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client = null;
let outputChannel = null;
let projectDiagnostics = null;
let statusBar = null;

const REQUIRED_META_FIELDS = [
  'nls',
  'custom_syscall_count',
  'game_mode',
  'game_mode_reserved',
  'game_title',
  'syscall_count',
  'syscalls',
];

function isFile(filePath) {
  if (!filePath) {
    return false;
  }
  try {
    return fs.statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function normalizeFsPath(uriOrPath) {
  if (!uriOrPath) {
    return '';
  }
  return typeof uriOrPath === 'string' ? uriOrPath : uriOrPath.fsPath;
}

function traceLevel() {
  const value = vscode.workspace.getConfiguration('luax').get('trace.server');
  switch (value) {
    case 'messages':
      return 1;
    case 'verbose':
      return 2;
    default:
      return 0;
  }
}

function bundledServerCandidates(context) {
  const serverDir = context.asAbsolutePath('server');
  const ext = process.platform === 'win32' ? '.exe' : '';
  const names = [];
  if (process.platform === 'darwin') {
    names.push(`luax-lsp-darwin-${process.arch}${ext}`);
  }
  if (process.platform === 'linux') {
    names.push(`luax-lsp-linux-${process.arch}${ext}`);
  }
  if (process.platform === 'win32') {
    names.push(`luax-lsp-windows-${process.arch}${ext}`);
  }
  names.push(`luax-lsp${ext}`);
  return names.map(name => path.join(serverDir, name));
}

function listWorkspaceFolders() {
  return vscode.workspace.workspaceFolders || [];
}

function findCargoWorkspaceRoot(startPath) {
  const resolved = normalizeFsPath(startPath);
  if (!resolved) {
    return null;
  }
  let current = fs.statSync(resolved).isDirectory() ? resolved : path.dirname(resolved);
  while (true) {
    const cargoToml = path.join(current, 'Cargo.toml');
    if (isFile(cargoToml)) {
      try {
        const content = fs.readFileSync(cargoToml, 'utf8');
        if (content.includes('[workspace]')) {
          return current;
        }
      } catch {
      }
    }
    const parent = path.dirname(current);
    if (parent === current) {
      break;
    }
    current = parent;
  }
  return null;
}

function hasWorkspaceCrate(root, crateName) {
  if (!root) {
    return false;
  }
  return isFile(path.join(root, 'crates', crateName, 'Cargo.toml'));
}

function findBestWorkspaceRoot() {
  for (const folder of listWorkspaceFolders()) {
    const root = findCargoWorkspaceRoot(folder.uri.fsPath) || folder.uri.fsPath;
    if (hasWorkspaceCrate(root, 'luax-lsp') || hasWorkspaceCrate(root, 'lua2hcb_compiler')) {
      return root;
    }
  }
  return null;
}

function resolveServerCommand(context) {
  const configured = vscode.workspace.getConfiguration('luax').get('server.path');
  if (configured && isFile(configured)) {
    return { command: configured, args: vscode.workspace.getConfiguration('luax').get('server.args') || [], cwd: undefined, source: 'setting' };
  }

  const envPath = process.env.LUAX_LSP_BIN;
  if (envPath && isFile(envPath)) {
    return { command: envPath, args: vscode.workspace.getConfiguration('luax').get('server.args') || [], cwd: undefined, source: 'environment' };
  }

  for (const candidate of bundledServerCandidates(context)) {
    if (isFile(candidate)) {
      return { command: candidate, args: vscode.workspace.getConfiguration('luax').get('server.args') || [], cwd: undefined, source: 'bundled' };
    }
  }

  const workspaceRoot = findBestWorkspaceRoot();
  if (workspaceRoot && hasWorkspaceCrate(workspaceRoot, 'luax-lsp')) {
    return {
      command: 'cargo',
      args: ['run', '-q', '-p', 'luax-lsp', '--', ...(vscode.workspace.getConfiguration('luax').get('server.args') || [])],
      cwd: workspaceRoot,
      source: 'cargo-workspace',
    };
  }

  return null;
}

function resolveCompilerCommand() {
  const configured = vscode.workspace.getConfiguration('luax').get('compiler.path');
  const configuredArgs = vscode.workspace.getConfiguration('luax').get('compiler.args') || [];
  if (configured && isFile(configured)) {
    return { command: configured, args: configuredArgs, cwd: undefined, source: 'setting' };
  }

  const envPath = process.env.LUAX_COMPILER_BIN;
  if (envPath && isFile(envPath)) {
    return { command: envPath, args: configuredArgs, cwd: undefined, source: 'environment' };
  }

  const workspaceRoot = findBestWorkspaceRoot();
  if (workspaceRoot && hasWorkspaceCrate(workspaceRoot, 'lua2hcb_compiler')) {
    return {
      command: 'cargo',
      args: ['run', '-q', '-p', 'lua2hcb_compiler', '--', ...configuredArgs],
      cwd: workspaceRoot,
      source: 'cargo-workspace',
    };
  }

  return null;
}

async function startClient(context) {
  const resolved = resolveServerCommand(context);
  if (!resolved) {
    const message = 'Luax language server not found. Set `luax.server.path`, define `LUAX_LSP_BIN`, bundle a binary under `server/`, or open the Rust workspace containing `crates/luax-lsp`.';
    outputChannel.appendLine(message);
    vscode.window.showErrorMessage(message, 'Open Settings').then(choice => {
      if (choice === 'Open Settings') {
        vscode.commands.executeCommand('workbench.action.openSettings', 'luax.server.path');
      }
    });
    return;
  }

  const serverOptions = {
    run: {
      command: resolved.command,
      args: resolved.args,
      transport: TransportKind.stdio,
      options: resolved.cwd ? { cwd: resolved.cwd } : undefined,
    },
    debug: {
      command: resolved.command,
      args: resolved.args,
      transport: TransportKind.stdio,
      options: resolved.cwd ? { cwd: resolved.cwd } : undefined,
    },
  };

  const clientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'luax' },
      { scheme: 'untitled', language: 'luax' },
    ],
    outputChannel,
    traceOutputChannel: outputChannel,
    synchronize: {
      configurationSection: 'luax',
      fileEvents: [
        vscode.workspace.createFileSystemWatcher('**/*.luax'),
        vscode.workspace.createFileSystemWatcher('**/*.yaml'),
        vscode.workspace.createFileSystemWatcher('**/*.yml'),
      ],
    },
  };

  client = new LanguageClient('luax', 'Luax Language Server', serverOptions, clientOptions);
  client.setTrace(traceLevel());
  vscode.commands.executeCommand('setContext', 'luax.serverRunning', true);
  client.start();
  await client.onReady();
  outputChannel.appendLine(`luax-lsp started via ${resolved.source}: ${resolved.command} ${resolved.args.join(' ')}`);
}

async function stopClient() {
  if (!client) {
    return;
  }
  const current = client;
  client = null;
  vscode.commands.executeCommand('setContext', 'luax.serverRunning', false);
  await current.stop();
}

async function restartClient(context) {
  await stopClient();
  await startClient(context);
}

function readDirectorySafely(dirPath) {
  try {
    return fs.readdirSync(dirPath, { withFileTypes: true });
  } catch {
    return [];
  }
}

function yamlCandidatesForDirectory(dirPath) {
  return readDirectorySafely(dirPath)
    .filter(entry => entry.isFile() && (entry.name.endsWith('.yaml') || entry.name.endsWith('.yml')))
    .map(entry => path.join(dirPath, entry.name));
}

function luaxCandidatesForDirectory(dirPath) {
  return readDirectorySafely(dirPath)
    .filter(entry => entry.isFile() && entry.name.endsWith('.luax'))
    .map(entry => path.join(dirPath, entry.name));
}

function parseMetaFile(metaPath) {
  const source = fs.readFileSync(metaPath, 'utf8');
  const data = yaml.load(source, { schema: yaml.FAILSAFE_SCHEMA });
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw new Error('Project YAML must be a mapping at the top level.');
  }
  return data;
}

function validateProjectMetaStructure(meta) {
  const errors = [];

  for (const field of REQUIRED_META_FIELDS) {
    if (!(field in meta)) {
      errors.push(`Missing required field: ${field}`);
    }
  }

  if (typeof meta.nls !== 'string') {
    errors.push('`nls` must be a string.');
  }
  if (!Number.isInteger(meta.custom_syscall_count)) {
    errors.push('`custom_syscall_count` must be an integer.');
  }
  if (!Number.isInteger(meta.game_mode)) {
    errors.push('`game_mode` must be an integer.');
  }
  if (!Number.isInteger(meta.game_mode_reserved)) {
    errors.push('`game_mode_reserved` must be an integer.');
  }
  if (typeof meta.game_title !== 'string') {
    errors.push('`game_title` must be a string.');
  }
  if (!Number.isInteger(meta.syscall_count)) {
    errors.push('`syscall_count` must be an integer.');
  }
  if (typeof meta.syscalls !== 'object' || meta.syscalls === null || Array.isArray(meta.syscalls)) {
    errors.push('`syscalls` must be a mapping from syscall id to { name, args }.');
    return errors;
  }

  const ids = Object.keys(meta.syscalls)
    .filter(key => /^\d+$/.test(key))
    .map(key => Number(key))
    .sort((a, b) => a - b);

  if (Number.isInteger(meta.syscall_count) && ids.length !== meta.syscall_count) {
    errors.push(`\`syscall_count\` is ${meta.syscall_count}, but \`syscalls\` contains ${ids.length} entries.`);
  }

  if (ids.length > 0) {
    for (let i = 0; i < ids.length; i += 1) {
      if (ids[i] !== i) {
        errors.push('`syscalls` ids must be contiguous and start at 0.');
        break;
      }
    }
  }

  for (const id of ids) {
    const entry = meta.syscalls[String(id)];
    if (typeof entry !== 'object' || entry === null || Array.isArray(entry)) {
      errors.push(`syscalls.${id} must be a mapping.`);
      continue;
    }
    if (typeof entry.name !== 'string' || entry.name.length === 0) {
      errors.push(`syscalls.${id}.name must be a non-empty string.`);
    }
    if (!Number.isInteger(entry.args) || entry.args < 0) {
      errors.push(`syscalls.${id}.args must be a non-negative integer.`);
    }
  }

  return errors;
}

function buildWholeDocumentDiagnostic(document, message) {
  const lineCount = Math.max(document.lineCount, 1);
  const lastLine = document.lineAt(lineCount - 1);
  return new vscode.Diagnostic(
    new vscode.Range(0, 0, lineCount - 1, lastLine.text.length),
    message,
    vscode.DiagnosticSeverity.Error,
  );
}

function validateProjectForLuaxFile(luaxPath) {
  const dir = path.dirname(luaxPath);
  const yamlCandidates = yamlCandidatesForDirectory(dir);
  const result = {
    ok: false,
    luaxPath,
    dir,
    metaPath: null,
    errors: [],
    meta: null,
  };

  if (yamlCandidates.length === 0) {
    result.errors.push('No project YAML file found in the same directory as this .luax file.');
    return result;
  }
  if (yamlCandidates.length > 1) {
    result.errors.push('More than one YAML file was found in the same directory. A legal Luax project must have exactly one project YAML file next to the .luax file.');
    return result;
  }

  result.metaPath = yamlCandidates[0];

  try {
    const meta = parseMetaFile(result.metaPath);
    result.meta = meta;
    result.errors.push(...validateProjectMetaStructure(meta));
  } catch (error) {
    result.errors.push(`Failed to parse project YAML: ${error.message}`);
  }

  result.ok = result.errors.length === 0;
  return result;
}

function applyProjectDiagnosticsForLuaxDocument(document) {
  const fsPath = normalizeFsPath(document.uri);
  if (!fsPath || document.languageId !== 'luax') {
    return;
  }

  const validation = validateProjectForLuaxFile(fsPath);
  const luaxDiags = validation.errors.map(message => buildWholeDocumentDiagnostic(document, message));
  projectDiagnostics.set(document.uri, luaxDiags);

  if (validation.metaPath && isFile(validation.metaPath)) {
    const metaUri = vscode.Uri.file(validation.metaPath);
    const metaDocPromise = vscode.workspace.openTextDocument(metaUri).catch(() => null);
    metaDocPromise.then(metaDoc => {
      if (!metaDoc) {
        return;
      }
      const metaDiags = validation.errors.map(message => buildWholeDocumentDiagnostic(metaDoc, message));
      projectDiagnostics.set(metaUri, metaDiags);
    });
  }

  updateStatusBarForValidation(validation);
}

function updateStatusBarForValidation(validation) {
  if (!statusBar) {
    return;
  }
  if (!validation) {
    statusBar.text = 'Luax: no project';
    statusBar.tooltip = 'Open a .luax file to validate the current project.';
    statusBar.command = 'luax.validateCurrentProject';
    return;
  }
  if (validation.ok) {
    statusBar.text = 'Luax: project OK';
    statusBar.tooltip = `Project YAML: ${path.basename(validation.metaPath)}`;
    statusBar.command = 'luax.openProjectYaml';
  } else {
    statusBar.text = `Luax: project invalid (${validation.errors.length})`;
    statusBar.tooltip = validation.errors.join('\n');
    statusBar.command = 'luax.validateCurrentProject';
  }
}

async function validateActiveEditorProject(showMessage) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'luax' || editor.document.uri.scheme !== 'file') {
    updateStatusBarForValidation(null);
    if (showMessage) {
      vscode.window.showErrorMessage('Open a .luax file first.');
    }
    return null;
  }

  const validation = validateProjectForLuaxFile(editor.document.uri.fsPath);
  applyProjectDiagnosticsForLuaxDocument(editor.document);

  if (showMessage) {
    if (validation.ok) {
      vscode.window.showInformationMessage(`Luax project is valid. Project YAML: ${path.basename(validation.metaPath)}`);
    } else {
      vscode.window.showErrorMessage(validation.errors[0]);
    }
  }

  return validation;
}

async function openProjectYaml() {
  const validation = await validateActiveEditorProject(false);
  if (!validation) {
    vscode.window.showErrorMessage('Open a .luax file first.');
    return;
  }
  if (!validation.metaPath) {
    vscode.window.showErrorMessage(validation.errors[0] || 'Project YAML not found.');
    return;
  }
  const doc = await vscode.workspace.openTextDocument(validation.metaPath);
  await vscode.window.showTextDocument(doc);
}

function resolveDefaultOutputUri(luaxPath) {
  const configured = vscode.workspace.getConfiguration('luax').get('compiler.defaultOutputDir');
  const fileName = `${path.basename(luaxPath, '.luax')}.hcb`;
  if (!configured) {
    return vscode.Uri.file(path.join(path.dirname(luaxPath), fileName));
  }

  const expanded = (() => {
    if (path.isAbsolute(configured)) {
      return configured;
    }
    const folders = listWorkspaceFolders();
    if (folders.length > 0) {
      return path.join(folders[0].uri.fsPath, configured);
    }
    return path.join(path.dirname(luaxPath), configured);
  })();

  return vscode.Uri.file(path.join(expanded, fileName));
}

function spawnAndCapture(command, args, cwd) {
  return new Promise((resolve, reject) => {
    outputChannel.appendLine(`> ${command} ${args.join(' ')}`);
    const child = cp.spawn(command, args, {
      cwd,
      shell: process.platform === 'win32',
      env: process.env,
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', chunk => {
      const text = chunk.toString();
      stdout += text;
      outputChannel.append(text);
    });

    child.stderr.on('data', chunk => {
      const text = chunk.toString();
      stderr += text;
      outputChannel.append(text);
    });

    child.on('error', error => {
      reject(error);
    });

    child.on('close', code => {
      resolve({ code, stdout, stderr });
    });
  });
}

async function compileCurrentProject(targetUri) {
  let luaxUri = targetUri;
  if (!luaxUri) {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'luax' || editor.document.uri.scheme !== 'file') {
      vscode.window.showErrorMessage('Open a .luax file first.');
      return;
    }
    luaxUri = editor.document.uri;
  }

  const validation = validateProjectForLuaxFile(luaxUri.fsPath);
  if (!validation.ok) {
    updateStatusBarForValidation(validation);
    vscode.window.showErrorMessage(validation.errors[0]);
    return;
  }

  const compiler = resolveCompilerCommand();
  if (!compiler) {
    vscode.window.showErrorMessage('Luax compiler not found. Set `luax.compiler.path`, define `LUAX_COMPILER_BIN`, or open the Rust workspace containing `crates/lua2hcb_compiler`.');
    return;
  }

  const outputUri = await vscode.window.showSaveDialog({
    saveLabel: 'Compile to HCB',
    defaultUri: resolveDefaultOutputUri(luaxUri.fsPath),
    filters: {
      'HCB bytecode': ['hcb'],
      'All files': ['*'],
    },
  });
  if (!outputUri) {
    return;
  }

  const args = [
    ...compiler.args,
    '--meta', validation.metaPath,
    '--lua', luaxUri.fsPath,
    '-o', outputUri.fsPath,
  ];

  outputChannel.show(true);
  const result = await spawnAndCapture(compiler.command, args, compiler.cwd).catch(error => ({ code: -1, stderr: error.message, stdout: '' }));
  if (result.code === 0) {
    vscode.window.showInformationMessage(`Luax project compiled successfully: ${outputUri.fsPath}`);
  } else {
    vscode.window.showErrorMessage(`Luax compilation failed with exit code ${result.code}. See the Luax output channel for details.`);
  }
}

function registerValidationHooks(context) {
  const refreshAll = () => {
    projectDiagnostics.clear();
    for (const doc of vscode.workspace.textDocuments) {
      if (doc.languageId === 'luax' && doc.uri.scheme === 'file') {
        applyProjectDiagnosticsForLuaxDocument(doc);
      }
    }
  };

  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(doc => {
    if (doc.languageId === 'luax' && doc.uri.scheme === 'file') {
      applyProjectDiagnosticsForLuaxDocument(doc);
    }
  }));

  context.subscriptions.push(vscode.workspace.onDidSaveTextDocument(doc => {
    if ((doc.languageId === 'luax' && doc.uri.scheme === 'file') || doc.fileName.endsWith('.yaml') || doc.fileName.endsWith('.yml')) {
      refreshAll();
    }
  }));

  context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
    if (!editor || editor.document.languageId !== 'luax' || editor.document.uri.scheme !== 'file') {
      updateStatusBarForValidation(null);
      return;
    }
    applyProjectDiagnosticsForLuaxDocument(editor.document);
  }));

  const watchers = [
    vscode.workspace.createFileSystemWatcher('**/*.yaml'),
    vscode.workspace.createFileSystemWatcher('**/*.yml'),
    vscode.workspace.createFileSystemWatcher('**/*.luax'),
  ];

  for (const watcher of watchers) {
    context.subscriptions.push(watcher);
    watcher.onDidCreate(refreshAll, null, context.subscriptions);
    watcher.onDidDelete(refreshAll, null, context.subscriptions);
    watcher.onDidChange(refreshAll, null, context.subscriptions);
  }

  refreshAll();
}

async function activate(context) {
  outputChannel = vscode.window.createOutputChannel('Luax');
  projectDiagnostics = vscode.languages.createDiagnosticCollection('luax-project');
  statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBar.show();

  context.subscriptions.push(outputChannel, projectDiagnostics, statusBar);

  context.subscriptions.push(vscode.commands.registerCommand('luax.restartServer', async () => {
    await restartClient(context);
    vscode.window.showInformationMessage('Luax language server restarted.');
  }));
  context.subscriptions.push(vscode.commands.registerCommand('luax.openProjectYaml', openProjectYaml));
  context.subscriptions.push(vscode.commands.registerCommand('luax.validateCurrentProject', async () => {
    await validateActiveEditorProject(true);
  }));
  context.subscriptions.push(vscode.commands.registerCommand('luax.compileCurrentProject', compileCurrentProject));

  registerValidationHooks(context);
  await startClient(context);
}

async function deactivate() {
  await stopClient();
}

module.exports = {
  activate,
  deactivate,
};
