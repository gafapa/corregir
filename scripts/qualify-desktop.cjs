// Windows desktop qualification uses the real installed executable and native IPC.
// Only the file picker is substituted; inference uses a local deterministic test server.
const { chromium } = require('playwright');
const { spawnSync } = require('node:child_process');
const { createServer } = require('node:http');
const { once } = require('node:events');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');

const executable = path.resolve(process.argv[2] || '');
const mode = process.argv[3] || 'create';
const outputDirectory = path.resolve(process.argv[4] || 'node_modules/.cache/desktop-qualification');
const workspace = path.join(process.env.APPDATA, 'com.corregir.qualification');
const root = path.resolve(__dirname, '..');
const port = 9437;
const backupPath = path.join(outputDirectory, 'synthetic.corregirbackup');
const password = 'synthetic qualification password';
fs.mkdirSync(outputDirectory, {recursive:true});
assert(fs.existsSync(executable), 'installed executable is missing');
const psQuote = value => "'" + value.replaceAll("'", "''") + "'";
function powershell(script) {
  const result = spawnSync('powershell.exe', ['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(script,'utf16le').toString('base64')], {encoding:'utf8',windowsHide:true});
  if (result.status !== 0) throw Error(result.stderr || result.stdout || 'PowerShell failed');
  return result.stdout.trim();
}
assert.equal(powershell(`(Get-Item -LiteralPath ${psQuote(executable)}).VersionInfo.ProductName`),'Corregir Qualification','refusing to launch a non-qualification application');
let processId, browser, page, applicationVersion;
let modelRequests = 0;
const server = createServer(async (request,response) => {
  let body = ''; for await (const chunk of request) { body += chunk; if (body.length > 100000) { response.writeHead(413).end(); return; } }
  try {
    const payload = JSON.parse(body); modelRequests++;
    assert.equal(request.url,'/api/chat');
    assert(!body.includes('Qualification Student'), 'real identity crossed the inference boundary');
    const result = payload.format?.properties?.evidence
      ? {evidence:[{criterion_id:'C1',evidence_textual:['nostalgia']}]}
      : {comment_feedback:'Synthetic feedback: review the textual evidence.',inconsistencies:[]};
    response.writeHead(200, {'Content-Type':'application/json'}).end(JSON.stringify({message:{content:JSON.stringify(result)}}));
  } catch(error) { response.writeHead(500).end(JSON.stringify({error:String(error)})); }
});
async function native(command,args={}) {
  return page.evaluate(({command,args}) => window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
}
async function picker(destination) {
  await native("cmd_set_qualification_picker", { path:destination });
}
async function closeApplication() {
  if (!processId) return;
  // Exercise Windows' normal close request without extending production IPC permissions.
  powershell(`$process = Get-Process -Id ${processId} -ErrorAction SilentlyContinue; if ($process) { if (!$process.CloseMainWindow()) { throw 'The qualification window did not accept a close request.' } }`);
  for(let attempt=0;attempt<80;attempt++) {
    if (powershell(`if (Get-Process -Id ${processId} -ErrorAction SilentlyContinue) { 'running' }`) !== 'running') { processId = null; return; }
    await new Promise(resolve => setTimeout(resolve,125));
  }
  throw Error('application did not close gracefully');
}
(async () => {
  if (mode === 'create') assert(!fs.existsSync(path.join(workspace,'corregir.sqlite.enc')), 'clean qualification requires a fresh isolated workspace');
  server.listen(0,'127.0.0.1'); await once(server,'listening');
  const modelUrl = `http://127.0.0.1:${server.address().port}`;
  processId = Number(powershell(`$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1'; $env:CORREGIR_QUALIFICATION_SOURCE_PATH=${psQuote(path.join(root,'synthetic-data','language-student-1.pdf'))}; $env:CORREGIR_QUALIFICATION_OUTPUT_ROOT=${psQuote(outputDirectory)}; $process = Start-Process -FilePath ${psQuote(executable)} -PassThru -WindowStyle Hidden; $process.Id`));
  for(let attempt=0;attempt<120;attempt++) {
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000}); break; } catch { await new Promise(resolve => setTimeout(resolve,500)); }
  }
  assert(browser, 'WebView2 debugging endpoint was not available');
  page = browser.contexts()[0].pages()[0]; page.setDefaultTimeout(20000);
  const errors=[]; page.on('pageerror',error=>errors.push(String(error)));
  await page.waitForFunction(() => Boolean(window.__TAURI_INTERNALS__));
  await page.getByRole('heading',{name:'Submissions',exact:true}).waitFor();
  assert.equal(await native('plugin:app|identifier'),'com.corregir.qualification');
  applicationVersion = await native('plugin:app|version');
  assert.equal(applicationVersion,['create','reopen'].includes(mode) ? '0.1.1' : '0.1.2');
  await native('cmd_save_ollama_settings',{settings:{url:modelUrl,model:'synthetic-qualification'}});
  if (mode === 'restore') {
    assert.equal((await native('cmd_list_assignments')).length,0);
    await native('cmd_verify_backup',{sourcePath:backupPath,password});
    await native('cmd_restore_backup',{sourcePath:backupPath,password});
  }
  if (mode === 'create') {
    await page.getByRole('button',{name:'Rubrics',exact:true}).click();
    await page.getByLabel('Title',{exact:true}).fill('Qualification rubric');
    await page.getByLabel('Subject',{exact:true}).fill('Language');
    await page.getByLabel('Year group',{exact:true}).fill('Synthetic');
    await page.getByLabel('Description 1',{exact:true}).fill('Find textual evidence for the theme.');
    await page.getByLabel('Maximum score 1',{exact:true}).fill('10');
    await page.getByRole('button',{name:'Save rubric',exact:true}).click();
    await page.getByText('Saved rubric #1.',{exact:false}).waitFor();
    await page.getByRole('button',{name:'Submissions',exact:true}).click();
    await page.getByLabel('Rubric:',{exact:false}).selectOption('1');
    await page.getByRole('button',{name:'Create assignment',exact:true}).click();
    await page.getByText('Active assignment: #1').waitFor();
    await picker(path.join(root,'synthetic-data','language-student-1.pdf'));
    await page.getByRole('button',{name:'Import document',exact:true}).click();
    await page.getByText('Submission imported and saved.',{exact:true}).waitFor();
    await page.getByText('Compare original and extracted text',{exact:true}).click();
    await page.getByRole('button',{name:'Select original document',exact:true}).click();
    await page.getByRole('img',{name:'Original document, page 1 of 1'}).waitFor();
    const originalText = await page.getByLabel('Extracted text (all pages)').inputValue();
    await page.getByLabel('Extracted text (all pages)').fill(originalText+'\nQualification Student');
    assert(await page.getByRole('button',{name:'Submissions',exact:true}).isDisabled());
    await page.getByRole('button',{name:'Save corrected text',exact:true}).click();
    await page.getByText('Corrected text saved.',{exact:false}).waitFor();
    await page.getByLabel("Student's real name",{exact:false}).fill('Qualification Student');
    await page.getByRole('button',{name:'Preview redacted text',exact:true}).click();
    await page.getByRole('checkbox',{name:'I checked this text for remaining identifiers.'}).check();
    await page.screenshot({path:path.join(outputDirectory,'document-review-desktop.png'),fullPage:true});
    await page.setViewportSize({width:600,height:850});
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth),false);
    await page.screenshot({path:path.join(outputDirectory,'document-review-narrow.png'),fullPage:true});
    await page.getByRole('button',{name:'Confirm redaction',exact:true}).click();
    await page.getByRole('button',{name:'Find evidence',exact:false}).click();
    await page.getByText('"nostalgia"',{exact:true}).waitFor();
    const score = page.getByRole('spinbutton').first(); await score.fill('8');
    await page.getByRole('button',{name:'Save my assessment',exact:false}).click();
    await page.getByRole('button',{name:'Generate feedback',exact:false}).click();
    await page.getByText('Synthetic feedback:',{exact:false}).waitFor();
    await page.getByRole('button',{name:'Confirm grade',exact:false}).click();
    await page.getByText('Grade confirmed for submission #1',{exact:false}).waitFor();
    await picker(path.join(outputDirectory,'grades.csv'));
    await page.getByRole('button',{name:'Export confirmed grades (CSV)',exact:true}).click();
    await page.getByText('Exported 1 confirmed grades',{exact:false}).waitFor();
    await picker(path.join(outputDirectory,'feedback.pdf'));
    await page.getByRole('button',{name:'Export feedback sheet (PDF)',exact:true}).click();
    await page.getByText('Feedback sheet saved',{exact:false}).waitFor();
    assert(fs.readFileSync(path.join(outputDirectory,'grades.csv'),'utf8').includes('Qualification Student'));
    assert.equal(fs.readFileSync(path.join(outputDirectory,'feedback.pdf')).subarray(0,5).toString(),'%PDF-');
    await native('cmd_export_backup',{destinationPath:backupPath,password});
    await native('cmd_verify_backup',{sourcePath:backupPath,password});
    assert.equal(modelRequests,2);
  }
  const submissions = await native('cmd_list_submissions',{assignmentId:1});
  assert.equal(submissions.length,1); assert.equal(submissions[0].status_pipeline,'grade_confirmed'); assert.equal(submissions[0].student_name,'Qualification Student');
  const grade = await native('cmd_load_grading_state',{submissionId:1});
  assert.equal(grade.criteria[0].score,8); assert(grade.feedback.comment_feedback.includes('Synthetic feedback'));
  assert.equal(grade.revision,1);
  assert.equal(errors.length,0,errors.join('\n'));
  if (mode === 'upgrade') {
    await page.getByText('Compare original and extracted text',{exact:true}).click();
    await picker(path.join(root,'synthetic-data','language-student-1.pdf'));
    await page.getByRole('button',{name:'Select original document',exact:true}).click();
    await page.getByRole('img',{name:'Original document, page 1 of 1'}).waitFor();
    await page.getByLabel('Original page zoom',{exact:false}).selectOption('200');
    assert(await page.locator('.page-viewport').evaluate(element => element.scrollWidth > element.clientWidth));
    await page.screenshot({path:path.join(outputDirectory,'document-review-final-desktop.png'),fullPage:true});
    await page.setViewportSize({width:600,height:850});
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth),false);
    await page.screenshot({path:path.join(outputDirectory,'document-review-final-narrow.png'),fullPage:true});
  }
  await closeApplication();
  assert(fs.existsSync(path.join(workspace,'corregir.sqlite.enc')));
  assert(!fs.existsSync(path.join(workspace,'corregir.sqlite')));
  assert(!fs.readFileSync(path.join(workspace,'corregir.sqlite.enc')).includes(Buffer.from('Qualification Student')));
  fs.writeFileSync(path.join(outputDirectory,`result-${mode}.json`),JSON.stringify({mode,passed:true,applicationVersion,modelRequests,submissions:1,score:8,workspace,executable},null,2));
  console.log(`Desktop qualification ${mode}: passed`);
})().catch(async error=>{ if (page && !page.isClosed()) { await page.screenshot({path:path.join(outputDirectory,`failure-${mode}.png`),fullPage:true}).catch(()=>{}); fs.writeFileSync(path.join(outputDirectory,`failure-${mode}.txt`),await page.locator('body').innerText().catch(()=>'')); } console.error(error);process.exitCode=1;}).finally(async()=>{
  if (processId) { try { await closeApplication(); } catch { powershell(`Stop-Process -Id ${processId} -Force -ErrorAction SilentlyContinue`); } }
  if (browser) await browser.close().catch(()=>{});
  server.close();
});
