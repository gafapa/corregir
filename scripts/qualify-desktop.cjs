// Windows desktop qualification uses the real installed executable and native IPC.
// Only the file picker and confirmation dialogs are substituted; inference uses a local
// deterministic test server.
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
const studentName = 'Qualification Student';
const classmateName = 'Second Synthetic Student';
const teacherComment = `${studentName} compared the theme with ${classmateName}.`;
const server = createServer(async (request,response) => {
  const chunks = []; let size = 0;
  for await (const chunk of request) { chunks.push(chunk); size += chunk.length; if (size > 100000) { response.writeHead(413).end(); return; } }
  const body = Buffer.concat(chunks).toString('utf8');
  try {
    const payload = JSON.parse(body); modelRequests++;
    assert.equal(request.url,'/api/chat');
    assert(!body.includes(studentName) && !body.includes(classmateName), 'real identity crossed the inference boundary');
    // The teacher comment reaches the model with the student's alias and the classmate hidden.
    if (!payload.format?.properties?.evidence) assert(body.includes('compared the theme with […].'), 'teacher comment was not redacted as expected');
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
// Sets the answer to the next confirmation dialogs; returns the messages asked before.
async function answer(confirm) {
  return native("cmd_set_qualification_answer", { confirm });
}
// Waits until the application asks, answering "cancel" meanwhile.
async function nextQuestion() {
  for (let attempt = 0; attempt < 80; attempt++) {
    const asked = await answer(false);
    if (asked.length > 0) return asked;
    await new Promise(resolve => setTimeout(resolve,125));
  }
  throw Error('no confirmation dialog was shown');
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
  // Selectors use the English interface; pin it regardless of the Windows display language.
  await page.evaluate(() => localStorage.setItem('corregir.language','en'));
  await page.reload();
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
    await page.getByRole('button',{name:'Classes',exact:true}).click();
    await page.getByLabel('Class name',{exact:true}).fill('Qualification class');
    await page.getByLabel('Students (one full name per line)',{exact:true}).fill(`${studentName}\n${classmateName}\n${studentName.toLowerCase()}`);
    await page.getByText('2 names. Duplicates and blank lines are ignored.',{exact:true}).waitFor();
    await page.getByRole('button',{name:'Save',exact:true}).click();
    await page.getByText('Class saved with 2 students.',{exact:true}).waitFor();
    await page.getByRole('button',{name:'Rubrics',exact:true}).click();
    await page.getByLabel('Title',{exact:true}).fill('Qualification rubric');
    await page.getByLabel('Subject',{exact:true}).fill('Language');
    await page.getByLabel('Year group',{exact:true}).fill('Synthetic');
    await page.getByLabel('Description 1',{exact:true}).fill('Find textual evidence for the theme.');
    await page.getByLabel('Maximum score 1',{exact:true}).fill('10');
    await page.getByText('Performance levels (0)',{exact:true}).click();
    await page.getByRole('button',{name:'Suggest four levels',exact:true}).click();
    await page.getByLabel('Level 2 score',{exact:true}).fill('8');
    await page.getByRole('button',{name:'Save rubric',exact:true}).click();
    await page.getByText('Saved rubric #1.',{exact:false}).waitFor();
    await page.getByRole('button',{name:'Submissions',exact:true}).click();
    await page.getByLabel('Assignment title',{exact:true}).fill('Qualification assignment');
    await page.getByLabel(/^Rubric/).selectOption('1');
    await page.getByLabel(/^Class roster/).selectOption({label:'Qualification class (2 students)'});
    await page.getByLabel('Assignment instructions',{exact:true}).fill('Explain the main theme of the poem.');
    await page.getByRole('button',{name:'Create assignment',exact:true}).click();
    await page.getByRole('heading',{name:'Qualification assignment',exact:true}).waitFor();
    await picker(path.join(root,'synthetic-data','language-student-1.pdf'));
    await page.getByRole('button',{name:'Import documents',exact:true}).click();
    await page.getByText('Imported and saved 1 of 1 documents.',{exact:true}).waitFor();
    await page.getByText('Compare original and extracted text',{exact:true}).click();
    await page.getByRole('button',{name:'Select original document',exact:true}).click();
    await page.getByRole('img',{name:'Original document, page 1 of 1'}).waitFor();
    const originalText = await page.getByLabel('Extracted text (all pages)').inputValue();
    await page.getByLabel('Extracted text (all pages)').fill(`${originalText}\n${studentName}`);
    assert(await page.getByRole('button',{name:'Submissions',exact:true}).isDisabled());
    await page.getByRole('button',{name:'Save corrected text',exact:true}).click();
    await page.getByText('Corrected text saved.',{exact:false}).waitFor();
    // Detection finds the roster name in the corrected text and suggests that student.
    await page.getByText('Suggested from the file name or the text. Check that it is correct.',{exact:true}).waitFor();
    assert.equal(await page.getByRole('combobox',{name:/^Student/}).inputValue(),'1');
    await page.getByText(`Name from the class list: "${studentName}"`,{exact:false}).waitFor();
    await page.getByRole('button',{name:'Preview redacted text',exact:true}).click();
    assert(!(await page.getByLabel('Redacted submission preview').innerText()).includes(studentName));
    await page.getByRole('checkbox',{name:'I checked this text for remaining identifiers.'}).check();
    await page.screenshot({path:path.join(outputDirectory,'document-review-desktop.png'),fullPage:true});
    await page.setViewportSize({width:600,height:850});
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth),false);
    await page.screenshot({path:path.join(outputDirectory,'document-review-narrow.png'),fullPage:true});
    await page.getByRole('button',{name:'Confirm redaction',exact:true}).click();
    await page.getByRole('button',{name:'1. Find evidence (no grade)',exact:true}).click();
    await page.getByText('"nostalgia"',{exact:true}).waitFor();
    await page.getByRole('group',{name:'Performance levels for C1'}).getByRole('button',{name:'Good · 8'}).click();
    assert.equal(await page.getByRole('spinbutton').first().inputValue(),'8');
    await page.getByText('Total: 8 / 10 points · Grade: 8 out of 10',{exact:true}).waitFor();
    await page.getByLabel('Teacher comment for C1:',{exact:false}).fill(teacherComment);
    await page.getByRole('button',{name:'Save my assessment',exact:false}).click();
    await page.getByRole('button',{name:'Generate feedback',exact:false}).click();
    // The AI draft is placed in the editable feedback field; confirming saves the teacher's text first.
    await page.waitForFunction(() => document.querySelector('.feedback-editor textarea')?.value.includes('Synthetic feedback:'));
    const feedbackEditor = page.locator('.feedback-editor textarea');
    await feedbackEditor.fill(`${await feedbackEditor.inputValue()} Qualification teacher edit.`);
    await page.getByRole('button',{name:'Confirm grade',exact:false}).click();
    await page.getByText('Grade confirmed:',{exact:false}).waitFor();
    await page.getByRole('button',{name:'Reopen grade',exact:true}).click();
    await page.getByLabel('Reason for reopening',{exact:false}).fill('Synthetic qualification check');
    await page.getByRole('button',{name:'Reopen for editing',exact:true}).click();
    await page.getByRole('button',{name:'Confirm grade',exact:false}).click();
    await page.getByText('Grade confirmed:',{exact:false}).waitFor();
    await picker(path.join(outputDirectory,'grades.csv'));
    await page.getByRole('button',{name:'Export confirmed grades (CSV)',exact:true}).click();
    await page.getByText('Exported 1 confirmed grades',{exact:false}).waitFor();
    await picker(path.join(outputDirectory,'feedback.pdf'));
    await page.getByRole('button',{name:'Export feedback sheet (PDF)',exact:true}).click();
    await page.getByText('Feedback sheet saved',{exact:false}).waitFor();
    const reports = path.join(outputDirectory,'reports'); fs.mkdirSync(reports);
    await picker(reports);
    await page.getByRole('button',{name:'Export all feedback sheets (PDF)',exact:true}).click();
    await page.getByText('Saved 1 feedback sheets in',{exact:false}).waitFor();
    const csv = fs.readFileSync(path.join(outputDirectory,'grades.csv'),'utf8');
    assert(csv.startsWith('﻿'), 'the spreadsheet has no byte-order mark');
    assert.deepEqual(csv.slice(1).trim().split(/\r?\n/),['Student,C1 (/10),Total score,Maximum,Grade (0-10)',`${studentName},8,8,10,8`]);
    assert.equal(fs.readFileSync(path.join(outputDirectory,'feedback.pdf')).subarray(0,5).toString(),'%PDF-');
    const batch = fs.readdirSync(reports); assert.equal(batch.length,1); assert(batch[0].endsWith('.pdf'));
    assert.equal(fs.readFileSync(path.join(reports,batch[0])).subarray(0,5).toString(),'%PDF-');
    // Deletion asks first: a cancelled dialog keeps the submission, a confirmed one removes it.
    await picker(path.join(root,'synthetic-data','language-student-1.pdf'));
    await page.getByRole('button',{name:'Import documents',exact:true}).click();
    await page.getByText('Imported and saved 1 of 1 documents.',{exact:true}).waitFor();
    assert.equal((await native('cmd_list_submissions',{assignmentId:1})).length,2);
    assert.equal(await page.locator('.submission-index [aria-current="true"]').count(),1);
    assert((await page.locator('.submission-index [aria-current="true"]').innerText()).includes('language-student-1.pdf'), 'the new submission is not selected');
    await answer(false);
    await page.getByRole('button',{name:'Delete submission',exact:true}).click();
    const asked = await nextQuestion();
    assert.equal(asked.length,1); assert(asked[0].startsWith('Delete language-student-1.pdf'), asked[0]);
    assert.equal((await native('cmd_list_submissions',{assignmentId:1})).length,2);
    await answer(true);
    await page.getByRole('button',{name:'Delete submission',exact:true}).click();
    await page.getByText('Submission deleted.',{exact:true}).waitFor();
    assert.equal((await answer(false)).length,1);
    await page.getByRole('button',{name:'Privacy',exact:true}).click();
    await page.getByLabel(/^Student/).selectOption(studentName);
    await page.getByText('1 submissions · 1 roster entries',{exact:true}).waitFor();
    await picker(path.join(outputDirectory,'student-data.json'));
    await page.getByRole('button',{name:'Export data (JSON)',exact:true}).click();
    await page.getByText('Exported 1 submissions to',{exact:false}).waitFor();
    const access = JSON.parse(fs.readFileSync(path.join(outputDirectory,'student-data.json'),'utf8'));
    assert.equal(access.format,'corregir-student-data/1'); assert.equal(access.submissions.length,1);
    await page.getByRole('combobox',{name:'Interface language'}).selectOption('gl');
    await page.getByRole('button',{name:'Entregas',exact:true}).click();
    await page.getByRole('heading',{name:'Entregas',exact:true}).waitFor();
    await page.screenshot({path:path.join(outputDirectory,'submissions-galician.png'),fullPage:true});
    await page.getByRole('combobox',{name:'Idioma da interface'}).selectOption('en');
    await page.getByRole('heading',{name:'Submissions',exact:true}).waitFor();
    await native('cmd_export_backup',{destinationPath:backupPath,password});
    await native('cmd_verify_backup',{sourcePath:backupPath,password});
    assert.equal(modelRequests,2);
  }
  const submissions = await native('cmd_list_submissions',{assignmentId:1});
  assert.equal(submissions.length,1); assert.equal(submissions[0].status_pipeline,'grade_confirmed'); assert.equal(submissions[0].student_name,studentName);
  assert.equal(submissions[0].student_id,1);
  const grade = await native('cmd_load_grading_state',{submissionId:1});
  assert.equal(grade.criteria[0].score,8); assert.equal(grade.criteria[0].comment_teacher,teacherComment);
  assert(grade.feedback.comment_feedback.includes('Synthetic feedback'));
  assert(grade.feedback_final.includes('Synthetic feedback') && grade.feedback_final.endsWith('Qualification teacher edit.')); assert.equal(grade.feedback_ai_assisted,true);
  assert.equal(grade.revision,1);
  const audit = (await native('cmd_list_logs_audit')).map(entry => entry.event);
  for (const event of ['class_saved','grade_reopened','feedback_saved','export_pdf_feedback','submission_deleted','student_data_exported']) assert(audit.includes(event), `missing audit event ${event}`);
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
  const snapshot = fs.readFileSync(path.join(workspace,'corregir.sqlite.enc'));
  assert(!snapshot.includes(Buffer.from(studentName)) && !snapshot.includes(Buffer.from(classmateName)));
  fs.writeFileSync(path.join(outputDirectory,`result-${mode}.json`),JSON.stringify({mode,passed:true,applicationVersion,modelRequests,submissions:1,score:8,workspace,executable},null,2));
  console.log(`Desktop qualification ${mode}: passed`);
})().catch(async error=>{ if (page && !page.isClosed()) { await page.screenshot({path:path.join(outputDirectory,`failure-${mode}.png`),fullPage:true}).catch(()=>{}); fs.writeFileSync(path.join(outputDirectory,`failure-${mode}.txt`),await page.locator('body').innerText().catch(()=>'')); } console.error(error);process.exitCode=1;}).finally(async()=>{
  if (processId) { try { await closeApplication(); } catch { powershell(`Stop-Process -Id ${processId} -Force -ErrorAction SilentlyContinue`); } }
  if (browser) await browser.close().catch(()=>{});
  server.close();
});
