<script lang="ts">
  import Database from '@tauri-apps/plugin-sql';
  import { onMount } from 'svelte';

  interface TaskRow {
    id: number;
    category: string | null;
    task: string;
    date: string | null;
    status: string;
  }

  let db: Database;
  let rows = $state<TaskRow[]>([]);

  let terminalInput = $state('');
  let previousInput = $state('');

  let currentView = $state<'listAll' | 'standard' | 'timely' | 'byCategory'>('standard');

  type ParsedCommand =
  | { command: 'add'; title: string; category?: string; date?: string }
  | { command: 'commit'; title: string, category?: string }
  | { command: 'push'; title: string, category?: string }
  | { command: 'stash' }
  | { command: 'switch'; view: string }
  | { command: 'rm'; title: string, category?: string }
  | { command: 'rmCategory'; category: string };

  function tokenize(input: string): string[] {
    const tokens: string[] = [];
    const regex = /"([^"]+)"|(\S+)/g;
    let match;

    while ((match = regex.exec(input)) !== null) {
      tokens.push(match[1] ?? match[2]);
    }

    return tokens;
  }

  function parseCommand(input: string): ParsedCommand | null {

    const tokens = tokenize(input.trim());

    if (tokens[0] !== 'git') return null;

    const cmd = tokens[1];

    if (cmd === 'add') {
      const title = tokens[2];
      const category = tokens[3];
      const date = tokens[4];
      if (!title) return null;
      return { command: 'add', title, category, date };
    }

    if (cmd === 'commit') {
      const title = tokens[2];
      const category = tokens[3];
      if (!title) return null;
      return { command: 'commit', title, category };
    }

    if (cmd === 'push') {
      const title = tokens[2];
      const category = tokens[3];
      if (!title) return null;
      return { command: 'push', title, category };
    }

    if (cmd === 'stash') {
      return { command: 'stash' };
    }

    if (cmd === 'switch') {
      const view = tokens[2];
      if (!view) return null;
      return { command: 'switch', view };
    }

    if (cmd === 'rm') {
      if (tokens[2] === '-c' || tokens[2] === '--category') {
        const category = tokens[3];
        if (!category) return null;
        return { command: 'rmCategory', category };
      }
      const title = tokens[2];
      const category = tokens[3];
      if (!title) return null;
      return { command: 'rm', title, category };
    }

    return null;
  }

  async function handleTerminalInput(e: KeyboardEvent) {

    if (e.key == 'ArrowUp') terminalInput = previousInput;
    if (e.key !== 'Enter') return;

    const parsed = parseCommand(terminalInput);
    previousInput = terminalInput;

    if (!parsed) return;

    if (parsed.command === 'add') { //add task

      if (parsed.category) await addCategory(parsed.category);

      let storedDate: string | null = null;

      if (parsed.date) {
        storedDate = parseDate(parsed.date);
        if (storedDate === null) return;
      }

      await addTask(parsed.title, parsed.category ?? null, storedDate);

    } else if (parsed.command === 'commit' || parsed.command === 'push') {

      if (parsed.category) {
        await db.execute(
          `UPDATE task SET status = $1
          WHERE task = $2
          AND id IN (
            SELECT task_id FROM connect
            WHERE category_id = (SELECT id FROM category WHERE category = $3)
          )`,
          [parsed.command, parsed.title, parsed.category]
        );
      } else {
        await db.execute(
          `UPDATE task SET status = $1 WHERE task = $2`,
          [parsed.command, parsed.title]
        );
      }

    } else if (parsed.command == 'stash') { //remove completed tasks

      await removeCompeleted();

    } else if (parsed.command == 'switch') { //switch views

      if (['listAll', 'standard', 'timely', 'byCategory'].includes(parsed.view)) {
        currentView = parsed.view as typeof currentView;
      }

    } else if (parsed.command == 'rm') { //remove task
        await removeTask(parsed.title, parsed.category);
    } else if (parsed.command === 'rmCategory') {
        await removeTasksByCategory(parsed.category);
    }

    await loadRows();
    terminalInput = ''; 

  }

  async function removeCompeleted() {
    await db.execute(
      `DELETE FROM connect WHERE task_id IN (SELECT id FROM task WHERE status = 'push')`
    );
    await db.execute(`DELETE FROM task WHERE status = 'push'`);
  }

  async function removeTasksByCategory(category: string) {
    const matches = await db.select<{ id: number }[]>(
      `SELECT task.id FROM task
      JOIN connect ON task.id = connect.task_id
      JOIN category ON connect.category_id = category.id
      WHERE category.category = $1`,
      [category]
    );

    if (matches.length === 0) return;

    for (const m of matches) {
      await db.execute(`DELETE FROM connect WHERE task_id = $1`, [m.id]);
      await db.execute(`DELETE FROM task WHERE id = $1`, [m.id]);
    }
  }

  async function addTask(title: string, category: string | null, date: string | null) {
    const result = await db.execute(
      'INSERT OR IGNORE INTO task (task, date, status) VALUES ($1, $2, $3)',
      [title, date, 'add']
    );

    let taskId: number | undefined = result.rowsAffected > 0 ? result.lastInsertId : undefined;

    if (taskId === undefined) {
      const existing = await db.select<{ id: number }[]>(
        'SELECT id FROM task WHERE task = $1',
        [title]
      );
      if (existing.length === 0) return;
      taskId = existing[0].id;
    }

    await db.execute(
      `INSERT OR IGNORE INTO connect (task_id, category_id)
      VALUES ($1, (SELECT id FROM category WHERE category = $2))`,
      [taskId, category]
    );
  }

  async function removeTask(title: string, category?: string) {
    let matches: { id: number }[];
    
    if (category) {
      matches = await db.select<{ id: number }[]>(
        `SELECT task.id FROM task
        JOIN connect ON task.id = connect.task_id
        JOIN category ON connect.category_id = category.id
        WHERE task.task = $1 AND category.category = $2`,
        [title, category]
      );
    } else {
      matches = await db.select<{ id: number }[]>(
        `SELECT id FROM task WHERE task = $1`,
        [title]
      );
    }

    if (matches.length === 0) return;

    const taskId = matches[0].id;

    await db.execute(`DELETE FROM connect WHERE task_id = $1`, [taskId]);
    await db.execute(`DELETE FROM task WHERE id = $1`, [taskId]);
  }

  async function addCategory(category: string) {
    await db.execute(
      'INSERT OR IGNORE INTO category (category) VALUES ($1)',
      [category]
    );
  }

  function formatDate(date: string | null): string {
    if (!date) return '';
    const datePart = date.split(' ')[0];
    const [, month, day] = datePart.split('-');
    return `${month}/${day}`;
  }

  function parseDate(input: string): string | null {
    const match = input.match(/^(\d{1,2})\/(\d{1,2})$/);
    if (!match) return null;

    const [, month, day] = match;
    const mm = month.padStart(2, '0');
    const dd = day.padStart(2, '0');

    const today = new Date();
    today.setHours(0, 0, 0, 0);

    let year = today.getFullYear();
    let candidate = new Date(`${year}-${mm}-${dd}T00:00:00`);

    if (candidate < today) {
      year += 1;
      candidate = new Date(`${year}-${mm}-${dd}T00:00:00`);
    }

    return `${year}-${mm}-${dd} 23:59:00`;
  }

  async function updateField(id: number, field: 'task' | 'date', value: string, inputEl?: HTMLInputElement) {
    let storedValue = value;

    if (field === 'date') {
      const converted = parseDate(value);
      if (converted === null) {
        const row = rows.find(r => r.id === id);
        if (row && inputEl) {
          inputEl.value = formatDate(row.date);
        }
        return;
      }
      storedValue = converted;
    }

    await db.execute(`UPDATE task SET ${field} = $1 WHERE id = $2`, [storedValue, id]);
    await loadRows();
  }

  function statusSymbol(status: string): string {
    switch(status) {
      case 'add': return '+';
      case 'commit': return '.';
      case 'push': return '✓';
      default: return status;
    }
  }

  function statusColor(status: string): string {
    switch(status) {
      case 'add': return "rgba(255,0,0,0.2)";
      case 'commit': return "rgba(255,255,0,0.2)";
      case 'push': return "rgba(0,255,0,0.2)";
      default: return "transparent";
    }
  }

  async function loadRows(){
    rows = await db.select<TaskRow[]>(`SELECT * FROM ${currentView}`);
  }

  onMount(async () => {
    db = await Database.load('sqlite:tasks.db');

    await loadRows();
  });

</script>

<main class="container">

  <div id="labels" data-tauri-drag-region>
    <h2 class="column-label" id="category-label">CATEGORY</h2>
    <h2 class="column-label" id="task-label">TASK</h2>
    <h2 class="column-label" id="date-label">DATE</h2>
    <h2 class="column-label" id="status-label">STATUS</h2>
  </div>

  <div id="table">
    {#each rows as row (row.id)}
      <div class="data-row">
        <span>{row.category ?? ''}</span>
        <span>
          <input 
            class="data-input" 
            bind:value={row.task}
            onchange={(e) => updateField(row.id, 'task', e.currentTarget.value)}
          >
        </span>
        <span>
          <input 
            class="data-input" 
            value={formatDate(row.date)}
            onchange={(e) => updateField(row.id, 'date', e.currentTarget.value, e.currentTarget)}
          >
        </span>
        <span style="background-color: {statusColor(row.status)}">
          {statusSymbol(row.status)}
        </span>
      </div>
    {/each}
  </div>

  <div id="terminal">
    <h3 id="terminal-starter">Task $</h3>
    <input 
      type="text" 
      id="terminal-box"
      bind:value={terminalInput}
      onkeydown={handleTerminalInput}
    >
  </div>

</main>

<style>

.container {
  border: 1px solid white;
  height: 100vh;
  padding: 1.2rem;
  
  font-family: var(--font-secondary);
  font-size: 0.8rem;
  letter-spacing: 0.01rem;
}

/* COLUMN LABELS */

#labels {
  display: grid;
  grid-template-columns: 1fr 3fr 0.7fr 0.5fr;
  user-select: none;
  cursor: pointer;
}

.column-label {
  font-size: 0.65rem;
  color: var(--grey-color);
  padding: 0.2rem 0.4rem;
  pointer-events: none;
}

/* MAIN CONTENT */

#table {
  height: 80vh;
  scrollbar-width: none;
  overscroll-behavior: none;
  overflow-y: auto;

  background: var(--light-background-color);
  border: 0.5px solid var(--light-grey-color);
}

.data-row {
  display: grid;
  grid-template-columns: 1fr 3fr 0.7fr 0.5fr;
  
  border-bottom: 0.5px solid var(--light-grey-color);
  padding-top: 0.2rem 0.4rem;

  white-space: nowrap;
}

.data-row span {
  text-overflow: ellipsis;
  padding: 0.2rem 0.4rem;
  overflow: hidden;

  border-right: 0.5px solid var(--grey-color);
}

.data-row span:last-child {
  border-right: none;
  text-align: center;
}

.data-input {
  background-color: transparent;
  outline: none;
  border: none;
  font-family: var(--font-secondary);
  font-size: 0.8rem;
  width: 100%;
}

.data-input:focus {
  outline: none;
}

/* FOOTER TERMINAL */

#terminal {
  margin-top: 0.5rem;
  background: var(--light-background-color);

  border: 1px solid var(--light-grey-color);
  display: flex;
}

h3 {
  font-family: var(--font-secondary);
  font-size: 0.8rem;
  font-weight: normal;
  
  padding-right: 0.4rem;
  margin-top: 0.2rem;
  margin-left: 0.2rem;

}

#terminal-box {
  width: 30rem;
  height: 1.3rem;

  background: none;
  border: none;

  font-family: var(--font-secondary);
  font-size: 0.8rem;
}

#terminal-box:focus {
  outline: none;
  color: var(--magenta-color);
}

</style>
