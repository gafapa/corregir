// Exercise the real normal portable ZIP with synthetic work and an isolated WebView2 profile.
const { chromium } = require('playwright');
const { spawn, spawnSync } = require('node:child_process');
const { randomUUID, createHash } = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const net = require('node:net');
const root = path.resolve(__dirname, '..');
const version = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;
const archive = path.resolve(process.argv[2] || path.join(root, 'output', 'portable', `Corregir_${version}_windows_x64_portable.zip`));
const output = path.join(root, 'output', 'work-directory-qualification', randomUUID());
const secret = 'synthetic directory password';
const backupPassword = 'synthetic independent backup password';
const title = 'Portable directory fixture';
const backup = path.join(output, 'transfer.corregirbackup');
const normalDatabase = path.join(process.env.APPDATA, 'com.corregir.desktop', 'corregir.sqlite.enc');
function fingerprint(file) { return fs.existsSync(file) ? createHash('sha256').update(fs.readFileSync(file)).digest('hex') : null; }
const normalBefore = fingerprint(normalDatabase);
const psQuote = value => "'" + value.replaceAll("'", "''") + "'";
function powershell(script) {
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')], { encoding: 'utf8', windowsHide: true });
  if (result.status !== 0) throw Error(result.stderr || result.stdout || 'PowerShell failed');
  return result.stdout.trim();
}
fs.mkdirSync(output, {recursive:true});
powershell(`Expand-Archive -LiteralPath ${psQuote(archive)} -DestinationPath ${psQuote(path.join(output, 'extracted'))}`);
const folders = fs.readdirSync(path.join(output, 'extracted'), {withFileTypes:true}).filter(item => item.isDirectory());
assert.equal(folders.length, 1);
let applicationFolder = path.join(output, 'extracted', folders[0].name);
let processId, browser, page, launches = 0;
const pageErrors = [];
let hostedPolicies = [];
async function native(command, args={}) { return page.evaluate(({command,args}) => window.__TAURI_INTERNALS__.invoke(command,args), {command,args}); }
function configureHostedBrowser(argumentsValue, directory) {
  if (process.env.GITHUB_ACTIONS !== 'true' || process.env.RUNNER_ENVIRONMENT !== 'github-hosted') return;
  // Elevated WebView2 hosts ignore debugging environment overrides starting with runtime 150.
  // Scope temporary machine policies to this executable on the disposable hosted runner only.
  const policies = [
    {key:'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2\\AdditionalBrowserArguments',value:argumentsValue},
    {key:'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2\\UserDataFolder',value:directory}
  ];
  const entries = policies.map(item => `@{Key=${psQuote(item.key)};Value=${psQuote(item.value)}}`).join(',');
  powershell(`$ErrorActionPreference='Stop'; $policies=@(${entries}); foreach ($policy in $policies) { if (Test-Path -LiteralPath $policy.Key) { if ((Get-ItemProperty -LiteralPath $policy.Key).PSObject.Properties['Corregir.exe']) { throw 'An existing Corregir WebView2 policy must not be overwritten.' } } }; $created=@(); try { foreach ($policy in $policies) { if (!(Test-Path -LiteralPath $policy.Key)) { New-Item -Path $policy.Key -Force | Out-Null }; New-ItemProperty -LiteralPath $policy.Key -Name 'Corregir.exe' -Value $policy.Value -PropertyType String | Out-Null; $created += $policy } } catch { foreach ($policy in $created) { Remove-ItemProperty -LiteralPath $policy.Key -Name 'Corregir.exe' }; throw }`);
  hostedPolicies = policies;
}
function restoreHostedBrowser() {
  if (!hostedPolicies.length) return;
  for (const policy of hostedPolicies) {
    powershell(`$ErrorActionPreference='Stop'; if (Test-Path -LiteralPath ${psQuote(policy.key)}) { $property=(Get-ItemProperty -LiteralPath ${psQuote(policy.key)}).PSObject.Properties['Corregir.exe']; if ($property) { if ($property.Value -ne ${psQuote(policy.value)}) { throw 'The temporary WebView2 policy changed unexpectedly.' }; Remove-ItemProperty -LiteralPath ${psQuote(policy.key)} -Name 'Corregir.exe' } }`);
  }
  hostedPolicies = [];
}
async function availablePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  return port;
}
async function launch() {
  const executable = path.join(applicationFolder, 'Corregir.exe');
  assert(applicationFolder.startsWith(output + path.sep));
  assert(fs.existsSync(path.join(applicationFolder, 'corregir-portable.json')));
  assert.equal(powershell(`(Get-Item -LiteralPath ${psQuote(executable)}).VersionInfo.ProductName`), 'Corregir');
  assert.equal(powershell(`(Get-Item -LiteralPath ${psQuote(executable)}).VersionInfo.ProductVersion`), version);
  const port = await availablePort();
  launches++;
  const browserDirectory = path.join(output, `webview-${launches}`);
  const stderr = path.join(output, `launch-${launches}.stderr.log`);
  const stdout = path.join(output, `launch-${launches}.stdout.log`);
  const browserArguments = `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1${process.env.CI ? ' --disable-gpu' : ''}`;
  configureHostedBrowser(browserArguments, browserDirectory);
  const outputHandles = [fs.openSync(stdout,'w'), fs.openSync(stderr,'w')];
  let launchError;
  try {
    const child = spawn(executable, [], { windowsHide:true, stdio:['ignore',...outputHandles], env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:browserDirectory,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:browserArguments} });
    child.once('error', error => { launchError = String(error); });
    processId = child.pid;
  } finally {
    for (const handle of outputHandles) fs.closeSync(handle);
  }
  assert(Number.isInteger(processId) && processId > 0);
  let connectionError;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {timeout:1000}); break; }
    catch (error) {
      connectionError = String(error);
      if (powershell(`if (Get-Process -Id ${processId} -ErrorAction SilentlyContinue) { 'running' }`) !== 'running') break;
      await new Promise(resolve => setTimeout(resolve,300));
    }
  }
  if (!browser) {
    const diagnostics = { process_id:processId, port, launch_error:launchError, connection_error:connectionError, browser_profile_created:fs.existsSync(browserDirectory), stderr:fs.existsSync(stderr) ? fs.readFileSync(stderr,'utf8') : '', stdout:fs.existsSync(stdout) ? fs.readFileSync(stdout,'utf8') : '' };
    fs.writeFileSync(path.join(output,'launch-failure.json'),JSON.stringify(diagnostics,null,2));
    console.error(JSON.stringify(diagnostics,null,2));
  }
  assert(browser, 'portable WebView2 endpoint did not open');
  page = browser.contexts()[0].pages()[0]; page.setDefaultTimeout(20000);
  page.on('pageerror', error => pageErrors.push(String(error)));
  // Selectors use the English interface; each launch has a fresh browser profile, so pin it every time.
  await page.waitForFunction(() => Boolean(window.__TAURI_INTERNALS__));
  await page.evaluate(() => localStorage.setItem('corregir.language','en'));
  await page.reload();
  await page.getByRole('heading', {name:'Open your work directory', exact:true}).waitFor();
  const status = await native('cmd_workspace_status');
  assert.equal(status.portable, true); assert.equal(status.opened, false);
  assert(status.directory.toLowerCase().startsWith(applicationFolder.toLowerCase()));
  assert.equal(await native('plugin:app|identifier'), 'com.corregir.desktop');
  assert.equal(await native('plugin:app|version'), version);
  await page.getByRole('button', {name:'Submissions', exact:true}).isDisabled().then(disabled => assert(disabled));
  assert(fs.existsSync(browserDirectory), 'isolated browser profile was not created');
}
async function close() {
  if (processId) {
    powershell(`$process = Get-Process -Id ${processId} -ErrorAction SilentlyContinue; if ($process -and !$process.CloseMainWindow()) { throw 'Portable window refused a normal close request.' }`);
    for (let attempt=0; attempt<50; attempt++) {
      if (powershell(`if (Get-Process -Id ${processId} -ErrorAction SilentlyContinue) { 'running' }`) !== 'running') { processId = null; break; }
      await new Promise(resolve => setTimeout(resolve,100));
    }
    assert.equal(processId, null, 'portable application did not close gracefully');
  }
  if (browser) await browser.close().catch(()=>{});
  browser = null; page = null;
  restoreHostedBrowser();
}
async function unlock(password, creating=false) {
  await page.getByLabel('Work-directory password', {exact:true}).fill(password);
  if (creating) await page.getByLabel('Confirm work-directory password', {exact:true}).fill(password);
  await page.getByRole('button', {name: creating ? 'Create work directory' : 'Unlock work directory', exact:true}).click();
}
async function opened() { await page.getByRole('heading', {name:'Submissions', exact:true}).waitFor(); }
async function assertRecords() {
  const rubrics = await native('cmd_list_rubrics'); assert.equal(rubrics.length,1); assert.equal(rubrics[0].title,title);
  const assignments = await native('cmd_list_assignments'); assert.equal(assignments.length,1); assert.equal(assignments[0].rubric_id,1);
}
(async () => {
  await launch();
  await page.getByRole('button',{name:'Create work directory',exact:true}).waitFor();
  await page.screenshot({path:path.join(output,'create-desktop.png'), fullPage:true});
  await page.setViewportSize({width:600,height:850});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth > document.documentElement.clientWidth),false);
  await page.screenshot({path:path.join(output,'create-narrow.png'), fullPage:true});
  await unlock(secret,true); await opened();
  await page.getByRole('button',{name:'Rubrics',exact:true}).click();
  await page.getByLabel('Title',{exact:true}).fill(title);
  await page.getByLabel('Subject',{exact:true}).fill('Science');
  await page.getByLabel('Year group',{exact:true}).fill('Synthetic');
  await page.getByLabel('Description 1',{exact:true}).fill('Preserve work while moving the directory.');
  await page.getByRole('button',{name:'Save rubric',exact:true}).click();
  await page.getByText('Saved rubric #1.',{exact:false}).waitFor();
  await page.getByRole('button',{name:'Submissions',exact:true}).click();
  await page.getByLabel(/^Rubric/).selectOption('1');
  await page.getByLabel('Assignment instructions',{exact:true}).fill('Synthetic assignment');
  await page.getByRole('button',{name:'Create assignment',exact:true}).click();
  await page.getByRole('heading',{name:'Synthetic assignment',exact:true}).waitFor();
  await assertRecords();
  await native('cmd_export_backup',{destinationPath:backup,password:backupPassword});
  await native('cmd_verify_backup',{sourcePath:backup,password:backupPassword});
  await close();
  const work = path.join(applicationFolder,'work');
  const database = path.join(work,'corregir.sqlite.enc');
  const encrypted = fs.readFileSync(database);
  assert(!encrypted.includes(Buffer.from(title))); assert(!fs.existsSync(path.join(work,'corregir.sqlite')));
  const key = fs.readFileSync(path.join(work,'corregir.workspace-key'));
  assert.equal(key.length,84); assert(!key.includes(Buffer.from(secret)));
  assert.equal(JSON.parse(fs.readFileSync(path.join(applicationFolder,'corregir-portable.json'),'utf8')).workspace,'work');
  await launch(); await unlock('incorrect synthetic password');
  await page.getByRole('alert').filter({hasText:'incorrect work-directory password'}).waitFor();
  assert.equal(await page.getByLabel('Work-directory password',{exact:true}).inputValue(),'');
  assert.deepEqual(fs.readFileSync(database),encrypted);
  await unlock(secret); await opened(); await assertRecords(); await close();
  const moved = path.resolve(output, 'Relocated application');
  assert(applicationFolder.startsWith(output + path.sep) && moved.startsWith(output + path.sep));
  fs.renameSync(applicationFolder,moved); applicationFolder = moved;
  await launch(); await unlock(secret); await opened(); await assertRecords();
  assert((await native('cmd_workspace_status')).directory.toLowerCase().includes('relocated application'));
  await page.screenshot({path:path.join(output,'relocated-open.png'),fullPage:true});
  await close();
  const originalSnapshot = fingerprint(path.join(applicationFolder,'work','corregir.sqlite.enc'));
  fs.writeFileSync(path.join(applicationFolder,'corregir-portable.json'),JSON.stringify({workspace:'Other work directory'}));
  await launch(); await unlock(secret,true); await opened();
  assert.equal((await native('cmd_list_rubrics')).length,0);
  await native('cmd_restore_backup',{sourcePath:backup,password:backupPassword});
  await assertRecords(); await close();
  assert.equal(fingerprint(path.join(applicationFolder,'work','corregir.sqlite.enc')),originalSnapshot);
  await launch(); await unlock(secret); await opened(); await assertRecords(); await close();
  assert.equal(fingerprint(normalDatabase),normalBefore,'the normal Windows-bound database changed during isolated qualification');
  assert.equal(pageErrors.length,0,pageErrors.join('\n'));
  fs.writeFileSync(path.join(output,'result.json'),JSON.stringify({passed:true,version,phases:['create','wrong-password','reopen','relocate','restore-new-directory','reopen-restored'],normal_database_unchanged:true,archive_sha256:fingerprint(archive),applicationFolder},null,2));
  console.log(`Portable work-directory qualification passed: ${output}`);
})().catch(async error => {
  if(page && !page.isClosed()) await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});
  console.error(error); process.exitCode=1;
}).finally(async()=>{
  try { await close(); } catch { if(processId) powershell(`$process=Get-Process -Id ${processId} -ErrorAction SilentlyContinue; if ($process) { Stop-Process -Id ${processId} -Force }`); }
  restoreHostedBrowser();
});
