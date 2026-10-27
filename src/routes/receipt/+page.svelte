<script lang="ts">

  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount, tick } from 'svelte';
  import Database from '@tauri-apps/plugin-sql';
  import { flip } from 'svelte/animate';

  import { writeTextFile, readTextFile, exists } from '@tauri-apps/plugin-fs';
  import { desktopDir, homeDir } from '@tauri-apps/api/path';

  interface Task {
    id: number;
    title: string;
    done: boolean;
    highlighted: boolean;
    fromTable?: boolean;
  }

  interface ReceiptTaskRow {
    id: number;
    task: string;
    status: string;
    highlighted: number;
  }

  interface UpcomingTableTask {
    id: number;
    task: string;
    date: string;
    status: 'add' | 'commit' | 'push';
    category: string | null;
  }

  let db: Database;
  let tasks = $state<Task[]>([]);

  type Mode = 'none' | 'highlight' | 'order';
  let mode = $state<Mode>('none');

  let selectedId = $state<number | null>(null);

  function toggleMode(m: Mode) {
    mode = mode === m ? 'none' : m;
    selectedId = null;
  }

  function handleRowClick(task: Task) {
    if (mode === 'highlight') {
      toggleHighlight(task);
    } else if (mode === 'order') {
      selectedId = selectedId === task.id ? null : task.id;
    }
  }

  async function toggleHighlight(task: Task) {
    task.highlighted = !task.highlighted;
    await db.execute(
      'UPDATE task SET highlighted = $1 WHERE id = $2',
      [task.highlighted ? 1 : 0, task.id]
    );
  }

  let inputRefs = $state<Record<number, HTMLInputElement>>({});

  function handleTaskKeyDown(e: KeyboardEvent, task: Task, index: number) {
    if ((e.key === 'Backspace' || e.key === 'Delete') && task.title === '') { // remove empty task
      if (e.repeat) return;  // ignore auto-repeat
      e.preventDefault();
      deleteTask(index);
      return;
    }

    if (e.key === 'Enter') { // add task to following index
      e.preventDefault();
      e.stopPropagation();
      addTask(index + 1);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (mode === 'order' && selectedId !== null) { // re-ordering task mode
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const i = tasks.findIndex(t => t.id === selectedId);
        moveTask(i, e.key === 'ArrowUp' ? -1 : 1);
        return;
      }
      if (e.key === 'Control') {
        selectedId = null;
        return;
      }
    }

    if (e.key === 'Control') { // add task to end of list
      addTask();
    }
  }

  let container: HTMLElement;

  async function updateWindowSize() {
    await tick();
    await new Promise(resolve => requestAnimationFrame(resolve));

    if (container) {
      await invoke('resize_window_to_content', { height: 1200 });
      await tick();
      await new Promise(resolve => requestAnimationFrame(resolve));

      const height = container.scrollHeight + 5;

      await invoke('resize_window_to_content', { height });
    }
  }

  async function updateTaskTitle(id: number, title: string) {
    await db.execute('UPDATE task SET task = $1 WHERE id = $2', [title, id]);
  }

  async function clearCompletedTasks() {
    const doneIds = tasks.filter(t => t.done).map(t => t.id);

    for (const id of doneIds) {
      await db.execute(`DELETE FROM task WHERE id = $1`, [id]);
    }

    tasks = tasks.filter(t => !t.done);
    await updateWindowSize();
    await savePositions();
  }

  function shortDate(dateStr: string): string {
    const [y, m, d] = dateStr.split('-');
    return `${m}/${d}`;
  }

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  async function exportTasks() {
    const upcomingTableTasks = await getUpcomingTableTasks();

    const taskRows = tasks
      .map((t, i) => `
        <li class="${t.done ? 'done' : ''} ${t.highlighted ? 'hl' : ''}">
          <span class="num">${i + 1}.</span>
          <span class="name">${escapeHtml(t.title)}</span>
          <span class="box">${t.done ? '✕' : ''}</span>
        </li>`)
      .join('');

    const STATUS_SYMBOL: Record<string, string> = { add: '+', commit: '•', push: '✓' };

    const tableRows = upcomingTableTasks.length
      ? upcomingTableTasks
          .map(t => `
        <tr class="st-${t.status}">
          <td>
            <div class="name">${escapeHtml(t.task)}</div>
            ${t.category ? `<div class="cat">${escapeHtml(t.category)}</div>` : ''}
          </td>
          <td class="date">${shortDate(t.date)}</td>
          <td class="status"><span class="box">${STATUS_SYMBOL[t.status] ?? ''}</span></td>
        </tr>`)
          .join('')
      : `<tr><td class="empty" colspan="3">nothing in the next 7 days</td></tr>`;

    const html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>mia's hit list</title>
<style>
  :root {
    --bg: #ffffff;
    --text: #111111;
    --muted: #888888;
    --line: #e5e5e5;
    --magenta: #d6338a;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #111111;
      --text: #f2f2f2;
      --muted: #8a8a8a;
      --line: #2a2a2a;
    }
  }
  * { box-sizing: border-box; }
  body {
    margin: 0 auto;
    max-width: 600px;
    padding: 20px 16px 32px;
    background: var(--bg);
    color: var(--text);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 14px;
    line-height: 1.35;
  }
  h1 { font-size: 24px; text-align: center; margin: 0 0 2px; }
  .today { text-align: center; color: var(--muted); font-size: 12px; margin: 0 0 6px; }
  h2 {
    font-size: 10px;
    letter-spacing: 0.08em;
    color: var(--muted);
    border-bottom: 1px dashed var(--muted);
    padding-bottom: 4px;
    margin: 20px 0 6px;
  }

  /* task list */
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 4px;
  }
  .num { color: var(--muted); min-width: 1.5em; font-variant-numeric: tabular-nums; }
  .name { flex: 1; word-break: break-word; }
  .box {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    border: 1px solid var(--text);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
  }
  .hl { background: rgba(214, 51, 138, 0.16); }
  .done .name { text-decoration: line-through; color: var(--muted); }

  /* coming up table */
  table { width: 100%; border-collapse: collapse; }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }
  td.date {
    width: 1%;
    white-space: nowrap;
    text-align: right;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  td.status { width: 1%; padding-left: 0; }
  td.empty { color: var(--muted); text-align: center; border-bottom: none; }
  .cat { font-size: 11px; color: var(--muted); margin-top: 1px; }
  .st-add .box    { background: rgba(255, 0, 0, 0.2); }
  .st-commit .box { background: rgba(255, 200, 0, 0.3); }
  .st-push .box   { background: rgba(0, 200, 0, 0.25); }
  .st-push .name  { text-decoration: line-through; color: var(--muted); }
  </style>
  </head>
    <body>
      <h1>mia's hit list</h1>
      <p class="today">${formatted}</p>

      <h2>TASKS</h2>
      <ul>${taskRows}
      </ul>

      <h2>COMING UP</h2>
      <table>${tableRows}
      </table>
    </body>
  </html>`;

    const desktop = await desktopDir();
    const filepath = `${desktop}/m/organization/hit-list.html`;

    await writeTextFile(filepath, html);
  }

  function localDateStr(d: Date): string {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `${y}-${m}-${day}`;
  }

  async function getUpcomingTableTasks(): Promise<UpcomingTableTask[]> {
    const today = new Date();
    const timeWindow = new Date();
    timeWindow.setDate(today.getDate() + 7);

    return await db.select<UpcomingTableTask[]>(
      `SELECT id, task, date(date) AS date, status, category FROM listAll
       WHERE date(date) BETWEEN date($1) AND date($2)`,
      [localDateStr(today), localDateStr(timeWindow)]
    );
  }

  //prevent removed edits
  const saveTimers: Record<number, ReturnType<typeof setTimeout>> = {};

  function queueTitleSave(id: number, title: string) {
    clearTimeout(saveTimers[id]);
    saveTimers[id] = setTimeout(() => updateTaskTitle(id, title), 300);
  }

  let importing = false;
  async function importInbox() {
    if (!db || importing) return;
    importing = true;

    try {
      const home = await homeDir();
      const path = `${home}/Library/Mobile Documents/iCloud~is~workflow~my~workflows/Documents/hit-list-inbox.txt`;
      if (!(await exists(path))) return;

      const lines = (await readTextFile(path))
        .split('\n')
        .map(l => l.trim())
        .filter(Boolean);
      if (lines.length === 0) return;

      await db.execute(`CREATE TABLE IF NOT EXISTS inbox_seen (key TEXT PRIMARY KEY)`);

      let position = tasks.length;
      const newTasks: Task[] = [];

      for (const line of lines) {
        const seen = await db.execute(
          `INSERT OR IGNORE INTO inbox_seen (key) VALUES ($1)`,
          [line]
        );
        if (seen.rowsAffected === 0) continue;

        const sep = line.indexOf('|');
        const title = sep > 0 ? line.slice(sep + 1).trim() : line;

        const res = await db.execute(
          `INSERT INTO task (task, date, status, source, position, highlighted)
          VALUES ($1, NULL, 'add', 'receipt', $2, 0)`,
          [title, position++]
        );
        if (res.lastInsertId !== undefined) {
          newTasks.push({ id: res.lastInsertId, title, done: false, highlighted: false });
        }
      }

      if (newTasks.length > 0) {
        tasks = [...tasks, ...newTasks];
        await updateWindowSize();
        await exportTasks();
      }
    } finally {
      importing = false;
    }
  }

  async function moveTask(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= tasks.length) return; //can't go past boundaries

    const reordered = [...tasks];
    [reordered[index], reordered[target]] = [reordered[target], reordered[index]]; //swap
    tasks = reordered;

    await savePositions();
  }

  async function savePositions() {
    for (let i = 0; i < tasks.length; i++) {
      await db.execute('UPDATE task SET position = $1 WHERE id = $2', [i, tasks[i].id]);
    }
  }

  async function addTask(index: number = tasks.length) {
    const result = await db.execute(
      `INSERT INTO task (task, date, status, source, position, highlighted)
       VALUES ($1, $2, $3, $4, $5, 0)`,
      ['', null, 'add', 'receipt', index]
    );

    const newId = result.lastInsertId;
    if (newId === undefined) return;

    const newTask: Task = { id: newId, title: '', done: false, highlighted: false };
    tasks = [...tasks.slice(0, index), newTask, ...tasks.slice(index)];

    if (index < tasks.length - 1) {
      await savePositions();
    }

    await tick();
    await updateWindowSize();
    inputRefs[newId]?.focus({ preventScroll: true });
  }

  async function deleteTask(index: number) {
    const task = tasks[index];

    // cancel potential title save for this task
    clearTimeout(saveTimers[task.id]);

    await db.execute('DELETE FROM task WHERE id = $1', [task.id]);
    tasks = tasks.filter(t => t.id !== task.id);

    await savePositions();
    await tick();
    await updateWindowSize();

    // move cursor above
    const prev = tasks[index - 1] ?? tasks[0];
    if (prev) {
      const el = inputRefs[prev.id];
      el?.focus({ preventScroll: true });
      el?.setSelectionRange(el.value.length, el.value.length); // cursor at end of text
    }
  }

  async function loadReceiptTasks() {
    const rows = await db.select<ReceiptTaskRow[]>(
      `SELECT id, task, status, highlighted FROM task 
       WHERE source = 'receipt'
       ORDER BY position, id`
    );

    tasks = rows.map(r => ({
      id: r.id,
      title: r.task,
      done: r.status === 'push',
      highlighted: r.highlighted === 1,
    }));
  }

  interface CalendarEvent {
    title: string;
    start: string;
    end: string;
  }

  let events = $state<CalendarEvent[]>([]);

  const calendarUrls = [
    "https://calendar.google.com/calendar/ical/miamochacha%40gmail.com/private-c6e36f1a8315f2032095c6023230702b/basic.ics", // home
    "https://calendar.google.com/calendar/ical/9996ebf413508951b0154e1d9b76d2d70b5404aae97697140fe154729eb648f2%40group.calendar.google.com/private-ac02781104196985a0dc8b737a925b2c/basic.ics", // mia & david
    "https://calendar.google.com/calendar/ical/9q8kbher2mqpsr0krj8t5vj5ms%40group.calendar.google.com/private-98a1ffe7d201b60cdb52498ae544bd13/basic.ics", // routines
    "https://calendar.google.com/calendar/ical/42li6ejbr4gqiaj3s72h1ohmnk%40group.calendar.google.com/private-dc41d314db7120ad3cb16e1feadefd9a/basic.ics", // special days
    "https://calendar.google.com/calendar/ical/mia.sravishankar%40gmail.com/private-6c3798bf2042ced78900c4e3e55ea352/basic.ics", // formal
    "https://calendar.google.com/calendar/ical/567501bde7b22acc55c61976671bc240beaedc3730d62b9808cf20b612b612aa%40group.calendar.google.com/private-28f022c5994e0e445bf935dfa9066697/basic.ics", // time planning
    "https://calendar.google.com/calendar/ical/msr2230%40columbia.edu/private-9fac2cc33a075d9e7080eb47835467c9/basic.ics", // school
    "https://calendar.google.com/calendar/ical/c_089c96f4f5182376df0d8ae68203d4cb80210f6ed775088f78452917d0b14a8b%40group.calendar.google.com/private-76a025b44b696ccae5e9dea644384ac0/basic.ics" // ecs
  ];

  function isAllDay(startStr: string): boolean {
    return startStr == "All day";
  }

  function getDayKey(startStr: string): string {
    if (isAllDay(startStr)) return startStr;
    return localDateStr(new Date(startStr));
  }

  function sortEvents(evts: CalendarEvent[]): CalendarEvent[] {
    return [...evts].sort((a, b) => {
      const dayA = getDayKey(a.start);
      const dayB = getDayKey(b.start);

      // different days
      if (dayA !== dayB) {
        return dayA < dayB ? -1 : 1;
      }

      // all-day first
      const aAllDay = isAllDay(a.start);
      const bAllDay = isAllDay(b.start);
      if (aAllDay !== bAllDay) {
        return aAllDay ? -1 : 1;
      }

      // sort by time
      return new Date(a.start).getTime() - new Date(b.start).getTime();
    });
  }

  async function loadEvents() {
    const fetched = await invoke<CalendarEvent[]>('fetch_calendar_events', { urls: calendarUrls });
    events = sortEvents(fetched);
  }

  function isPast(endTime: string): boolean {
    return new Date(endTime).getTime() < Date.now();
  }

  onMount(() => {
    loadEvents();

    const win = getCurrentWindow();
    now = new Date();

    const unlisten = win.listen('tauri://focus', async () => {
      now = new Date();
      await importInbox();
      await loadEvents();
      await updateWindowSize();
      await exportTasks();
    });

    (async () => {
      db = await Database.load('sqlite:tasks.db');
      await loadReceiptTasks();
      await importInbox();
      await loadEvents();
      await updateWindowSize();
      await exportTasks();
    })(); 

    return () => {
      unlisten.then((fn) => fn());
    };
  })

  let now = $state(new Date());
  
  const formatted = $derived(
    now.toLocaleDateString('en-US', {
      month: '2-digit',
      day: '2-digit',
      year: 'numeric'
    })
  );
</script>

<svelte:window onkeydown={handleKeyDown} />
<main class="container" bind:this={container}>

  <div class="header" data-tauri-drag-region>
    <h1 id="title">mia's hit list</h1>
    <p id="date">{formatted}</p>
  </div>

  <div class="schedule">
    <h2 class="section-title">SCHEDULE</h2>
    {#each events as event}
    <div class="event" class:past={isPast(event.end)}>
      <span class="event-title">{event.title}</span>
      <span class="event-time">{event.start}</span>
    </div>
  {/each}
  </div>

  <div class="tasks">
    <h2 class="section-title">TASKS</h2>
    <div class=task-items>
      {#each tasks as task, i (task.id)}
        <div
          class="task-display"
          class:highlight-mode={mode === 'highlight'}
          class:highlighted={task.highlighted}
          class:order-mode={mode === 'order'}
          class:selected={task.id === selectedId}
          animate:flip={{ duration: 180 }}
          onclick={() => handleRowClick(task)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleRowClick(task); }}
          role="button"
          tabindex="0"
        >
          <div class="task-text-display">
            <p class="order">{i + 1}.</p>
            <input
              type="text"
              class:done={task.done}
              bind:value={task.title}
              bind:this={inputRefs[task.id]}
              disabled={mode !== 'none'}
              oninput={(e) => queueTitleSave(task.id, e.currentTarget.value)}
              onchange={(e) => updateTaskTitle(task.id, e.currentTarget.value)}
              onkeydown={(e) => handleTaskKeyDown(e, task, i)}
            />
          </div>
          <input
            type="checkbox"
            bind:checked={task.done}
            disabled={mode !== 'none'}
            onchange={() => db.execute(
              'UPDATE task SET status = $1 WHERE id = $2',
              [task.done ? 'push' : 'add', task.id]
            )}
          />
        </div>
      {/each}
    </div>
  </div>

  <div class="footer">

    <div id="extra">
      <p id="commands">
        <button 
          id="order" 
          onclick={() => toggleMode('order')}
          class:active={mode === 'order'}>
          Order
        </button>,
        <button 
          id="highlight" 
          onclick={() => toggleMode('highlight')}
          class:active={mode === 'highlight'}>
          Highlight
        </button>,
        <button class="hover-buttons" onclick={clearCompletedTasks}>Clear</button>
      </p>
    </div>

  </div>
</main>

<style>

  /* OVERALL */
  .container {
    border: 1px solid white;
    padding: 1.2rem;

    min-height: 150px;
    /* max-height: 800px;
    /* overflow-y: auto; */

    font-family: var(--font-secondary);
    font-size: 0.8rem;
    letter-spacing: 0.01rem;

    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  /* HEADER */
  .header {
    border-bottom: 1px dashed var(--grey-color);
    padding-bottom: 0.3rem;
  }

  #title {
    text-align: center;
    font-size: 2rem;
    font-family: var(--font-primary);
    margin-bottom: 0.5rem;
    user-select: none;
    pointer-events: none;
  }

  #date {
    font-size: 0.8rem;
  }

  /* MAIN CONTENT */
  h2 {
    font-size: 0.65rem;
    color: var(--grey-color);
    padding-bottom: 0.4rem;
  }

  .schedule {
    border-bottom: 1px solid var(--grey-color);
    padding-bottom: 0.6rem;
  }

  .tasks {
    overflow-y: auto;
    flex: 1;
    border-bottom: 1px dashed var(--grey-color);
    padding-bottom: 0.3rem;
  }

  .task-items {
    display: flex;
    flex-direction: column;
  }

  input[type="text"] {
    border: none;
    width: 17rem;
    background: transparent;
    font-family: var(--font-secondary);
    font-size: 0.8rem;
  }

  input[type="text"]:focus {
    outline: none;
    color: var(--magenta-color);
  }

  input[type="checkbox"] {
    appearance: none;
    width: 0.7rem;
    height: 0.7rem;
    cursor: pointer;
  }

  input[type="checkbox"] {
    border: 1px solid black;
    position: relative;
  }

  input[type="checkbox"]:hover {
    border-color: var(--magenta-color);
  }

  input[type="checkbox"]:checked::after {
    border-color: 1px solid var(--light-grey-color);
    content: 'X';
    position: absolute;
    top: -0.14rem;
    left: 0.07rem;
  }

  .task-text-display {
    display: flex;
    gap: 0.2rem;
  }

  .done {
    text-decoration: line-through;
    color: var(--light-grey-color);
  }

  .task-display.remove-mode {
    cursor: pointer;
  }

  #highlight.active, #order.active {
    color: var(--magenta-color);
  }

  .task-display.remove-mode input[type="text"]:disabled {
    color: var(--magenta-color);
  }

  input:disabled {
    pointer-events: none;
  }

  /* FOOTER */

  #extra, .event, .task-display {
    display: flex;
    justify-content: space-between;
  }

  button {
    border: none;
    background: transparent;
    font-family: var(--font-secondary);
    font-size: 0.8rem;
  }

  .hover-buttons:hover {
    color: var(--magenta-color);
  }

  .task-display.highlighted {
    background: color-mix(in srgb, var(--magenta-color) 14%, transparent);
  }

  .task-display.highlight-mode {
    cursor: pointer;
  }

  .task-display.highlight-mode:hover {
    background: color-mix(in srgb, var(--magenta-color) 8%, transparent);
  }

  .task-display.order-mode {
    cursor: pointer;
  }

  .task-display.order-mode:hover {
    background: color-mix(in srgb, var(--grey-color) 10%, transparent);
  }

  .task-display:focus {
    outline: none;
    user-select: none;
  }

  .task-display.selected input[type="text"]:disabled {
    color: var(--magenta-color);
  }

</style>