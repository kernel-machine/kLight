import { expect, test, type Page } from '@playwright/test';

async function runtime(page: Page) {
  const r = await page.request.get('/api/runtime');
  return r.json();
}

test('patch, program, blackout and grand master', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();

  // Patch two dimmers from the library, picked by manufacturer then model.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');
  const addressField = page.getByLabel('Address');
  await expect(addressField).toHaveValue('1');
  await page.getByLabel('Count').fill('2');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Dimmer 2' })).toBeVisible();
  // The next suggestion is the first free address after the patched dimmers.
  await expect(addressField).toHaveValue('3');

  // Several multi-channel fixtures land one after the other.
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await expect(addressField).toHaveValue('3');
  // The channels of the selected mode are listed.
  const channels = page.getByLabel('Mode channels').locator('tbody tr');
  await expect(channels).toHaveCount(11);
  await expect(channels.nth(1)).toContainText('Pan Fine');
  await page.getByLabel('Count').fill('3');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  for (const [n, range] of [[1, '3–13'], [2, '14–24'], [3, '25–35']] as const) {
    await expect(page.locator('.tree li', { hasText: `Spot 540 ${n}` })).toContainText(range);
  }
  await expect(addressField).toHaveValue('36');
  await page.getByLabel('Count').fill('1');

  // QLC+ definitions are in the library, sorted by brand (when downloaded:
  // scripts/update-qlcplus-fixtures.sh; CI always downloads them).
  const library = (await (await page.request.get('/api/library')).json()) as Array<{ manufacturer: string }>;
  if (library.some((e) => e.manufacturer === 'Martin')) {
    await page.getByPlaceholder('Search manufacturer or model…').fill('Martin');
    await expect(page.getByLabel('Manufacturer').locator('option').first()).toContainText('Martin');
    await page.getByPlaceholder('Search manufacturer or model…').fill('');
  } else {
    console.warn('QLC+ library not downloaded: brand search not tested');
  }
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');

  // A typed conflicting address is flagged before sending.
  await addressField.fill('1');
  await expect(page.getByText(/Conflicts with/)).toBeVisible();
  await page.getByRole('button', { name: 'First free' }).click();
  await expect(addressField).toHaveValue('36');

  // Simple Desk: select a fixture and set it to full.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Dimmer 1', exact: true }).click();
  await page.getByRole('button', { name: 'Full' }).click();
  await expect(page.getByText('Intensity 100%')).toBeVisible();
  await expect.poll(async () => (await runtime(page)).state.programmer.length).toBe(1);

  // Save the programmer as a scene: it appears in the FX screen.
  page.once('dialog', (d) => d.accept('Look 1'));
  await page.getByRole('button', { name: 'Save as scene' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await expect(page.getByText('Look 1')).toBeVisible();

  // Blackout and grand master are always reachable.
  await page.getByRole('button', { name: 'Blackout' }).click();
  await expect(page.getByRole('button', { name: 'BLACKOUT ON' })).toBeVisible();
  await expect.poll(async () => (await runtime(page)).state.blackout).toBe(true);
  await page.getByRole('button', { name: 'BLACKOUT ON' }).click();
  await expect.poll(async () => (await runtime(page)).state.blackout).toBe(false);
});

test('scene editor, movement preview, knob and DMX channel desk', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();

  // Select heads in the universe tree and create a group.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Select Dimmer 1').check();
  await page.getByLabel('Select Dimmer 2').check();
  page.once('dialog', (d) => d.accept('Front'));
  await page.getByRole('button', { name: 'Create group' }).click();
  // The group is a folder of the universe tree holding its fixtures, and
  // opens its editor.
  const front = page.getByRole('treeitem', { name: 'Group Front' });
  await expect(front).toBeVisible();
  await expect(front).toHaveAttribute('aria-selected', 'true');
  await expect(front).toContainText('2');

  // Reorder the group by dragging heads in the matrix.
  await expect(page.getByLabel('Head Dimmer 1')).toContainText('1');
  await page.getByLabel('Head Dimmer 1').dragTo(page.getByLabel('Head Dimmer 2'));
  await expect(page.getByLabel('Head Dimmer 2')).toContainText('1');
  await expect(page.getByLabel('Head Dimmer 1')).toContainText('2');

  // Settings depend on the fixture type: a dimmer has no orientation.
  await page.locator('.tree li', { hasText: 'Dimmer 1' }).first().locator('button.fx').click();
  await expect(page.getByRole('heading', { name: 'Channels' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Orientation' })).toHaveCount(0);

  // A second group, of moving heads.
  await page.getByLabel('Select Spot 540 1').check();
  await page.getByLabel('Select Spot 540 2').check();
  page.once('dialog', (d) => d.accept('Beams'));
  await page.getByRole('button', { name: 'Create group' }).click();
  await expect(page.getByRole('treeitem', { name: 'Group Beams' })).toBeVisible();

  // Add a group to a scene from the scene editor.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Front' });
  await page.getByLabel('Scene attribute').selectOption('intensity');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: 'Intensity 100%' })).toBeVisible();

  // Only what the fixtures can do is offered: dimmers have no zoom, strobe,
  // position or color; moving heads do.
  const attribute = page.getByLabel('Scene attribute');
  await expect(attribute.locator('option')).toHaveText(['Intensity']);
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Beams' });
  await expect(attribute.locator('option')).toHaveText([
    'Intensity', 'Color', 'Position', 'Fan (group order)', 'Gobo', 'Prism', 'Zoom', 'Strobe',
  ]);

  // Regression: quick successive additions must all be kept (each used to
  // be computed on a stale copy of the scene and overwrite the previous one).
  await page.getByLabel('Scene attribute').selectOption('zoom');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await page.getByLabel('Scene attribute').selectOption('strobe');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await page.getByLabel('Scene attribute').selectOption('position');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  for (const text of ['Intensity 100%', 'Zoom 20°', 'Strobe 5 Hz', 'Pan 0.0° Tilt 0.0°']) {
    await expect(page.locator('td', { hasText: text })).toBeVisible();
  }

  // The scene lists the fixtures it drives and their channel values.
  const preview = page.locator('.card', { hasText: 'Dimmer 1' });
  await expect(page.getByText('Fixtures in this scene (4)')).toBeVisible();
  await expect(preview.locator('tbody tr').first()).toContainText('255');
  await page.getByRole('treeitem', { name: /Scene 2/ }).getByRole('button', { name: 'Start' }).click();
  await expect(preview.locator('tbody tr .now').first()).toHaveText('255');
  await expect.poll(async () => {
    const show = await (await page.request.get('/api/runtime')).json();
    return show.seq;
  }).toBeGreaterThan(0);

  // A fan on a group is an intention resolved by the core.
  await page.getByLabel('Scene attribute').selectOption('fan');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: /^Fan 60° linear/ })).toBeVisible();

  // Movement effect: the drawing follows the values.
  await page.getByRole('button', { name: 'New movement', exact: true }).click();
  const shape = page.locator('svg[aria-label="Movement preview"] path');
  await expect(shape).toBeVisible();
  const circle = await shape.getAttribute('d');
  await page.getByRole('button', { name: 'eight' }).click();
  await expect.poll(() => shape.getAttribute('d')).not.toBe(circle);
  const before = await shape.getAttribute('d');
  await page.locator('label.slider', { hasText: 'Width' }).locator('input[type=number]').fill('200');
  await page.locator('label.slider', { hasText: 'Width' }).locator('input[type=number]').press('Enter');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(before);

  // Simple Desk: every channel of the universe can be set on its own.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'DMX channels' }).click();
  await expect(page.getByLabel('Channel 512', { exact: true })).toBeAttached();
  await page.getByLabel('Channel 300 value').fill('77');
  await page.getByLabel('Channel 300 value').press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels)
    .toContainEqual({ universe: 1, address: 300, value: 77 });
  await page.getByRole('button', { name: 'Release universe' }).click();
  await expect.poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels).toEqual([]);

  // Virtual console: a knob is a rotary control.
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.getByRole('button', { name: 'Knob' }).click();
  await expect(page.getByRole('slider', { name: 'Knob' })).toBeVisible();

  // Matrix effect preview shows one cell per head of the group.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New matrix effect', exact: true }).click();
  await expect(page.locator('.matrix .cell')).toHaveCount(2);

  // DMX monitor names fixtures and channels.
  await page.getByRole('button', { name: 'DMX monitor' }).click();
  await expect(page.locator('.monitor .cell').first()).toContainText('Dimmer 1');
  await page.getByRole('button', { name: 'Close' }).click();

  // Deleting a universe deletes its fixtures too.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByRole('button', { name: '+ Universe' }).click();
  await expect(page.getByText('2: Universe 2')).toBeVisible();
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');
  await page.locator('section', { hasText: 'Add fixtures' }).getByLabel('Universe').selectOption('2');
  await page.getByPlaceholder('Name (default: model)').fill('Far dimmer');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Far dimmer' })).toBeVisible();
  page.once('dialog', (d) => d.accept());
  await page.getByLabel('Delete universe Universe 2').click();
  await expect(page.getByText('2: Universe 2')).toHaveCount(0);
  await expect(page.locator('.tree li', { hasText: 'Far dimmer' })).toHaveCount(0);
});

test('functions are listed by type with nestable folders', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const tree = page.getByRole('tree');
  await expect(tree.getByText('Scenes', { exact: true })).toBeVisible();
  // Types without anything in them are not listed.
  await expect(tree.getByText('Chasers', { exact: true })).toHaveCount(0);
  await expect(tree.getByText('Cue lists', { exact: true })).toHaveCount(0);
  // A folder alone makes its type appear.
  page.once('dialog', (d) => d.accept('Songs'));
  await page.getByLabel('New folder in').selectOption('cue_list');
  await expect(tree.getByText('Cue lists', { exact: true })).toBeVisible();

  // A folder in Scenes, a subfolder inside it, and a new scene in the subfolder.
  page.once('dialog', (d) => d.accept('Act 1'));
  await page.getByRole('button', { name: 'New scene folder' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toBeVisible();
  page.once('dialog', (d) => d.accept('Song A'));
  await page.getByRole('button', { name: 'New subfolder in Act 1' }).click();
  await expect(page.getByLabel('Folder Song A', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'New function in Song A' }).click();
  const editorFolder = page.getByLabel('Folder', { exact: true });
  await expect(editorFolder.locator('option:checked')).toHaveText('Act 1 / Song A');

  // Regression: two creations in a row must not reuse the same id (the
  // second used to overwrite the first before the show update arrived).
  await page.getByRole('button', { name: 'New function in Act 1' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toContainText('2');
  await expect(page.getByLabel('Folder Song A', { exact: true })).toContainText('1');

  // Drag an existing scene ("Look 1", top level) into "Act 1".
  await tree.getByRole('treeitem', { name: /Look 1/ }).dragTo(page.getByLabel('Folder Act 1', { exact: true }));
  await tree.getByRole('treeitem', { name: /Look 1/ }).click();
  await expect(editorFolder.locator('option:checked')).toHaveText('Act 1');

  // Deleting "Act 1" moves its content up one level.
  page.once('dialog', (d) => d.accept());
  await page.getByRole('button', { name: 'Delete folder Act 1' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Folder Song A', { exact: true })).toBeVisible();
  await tree.getByRole('treeitem', { name: /Look 1/ }).click();
  await expect(editorFolder.locator('option:checked')).toHaveText('— top level —');
});


// Regressions: a dropped widget flashed back to its start for one round trip
// to the engine, and the clock widget never ticked.
test('virtual console: a dropped widget stays put and the clock ticks', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const toolbar = page.locator('.toolbar');

  await toolbar.getByRole('button', { name: 'Clock', exact: true }).click();
  const clock = page.locator('.w.clock .big').last();
  const shown = await clock.textContent();
  await expect.poll(() => clock.textContent(), { timeout: 3000 }).not.toBe(shown);

  await toolbar.getByRole('button', { name: 'Label', exact: true }).click();
  const w = page.locator('.w.label').last();
  const styleLeft = () => w.evaluate((el) => parseFloat((el as HTMLElement).style.left));
  const start = await styleLeft();
  const box = (await w.boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 85, box.y + 5, { steps: 5 });
  // Record where the widget is drawn on every frame from just before the drop.
  await w.evaluate((el) => {
    const seen: number[] = [];
    (window as unknown as { lefts: number[] }).lefts = seen;
    const frame = () => {
      seen.push(el.getBoundingClientRect().left);
      if (seen.length < 60) requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  });
  await page.mouse.up();
  await page.waitForFunction(() => (window as unknown as { lefts: number[] }).lefts.length >= 60);
  const lefts = await page.evaluate(() => (window as unknown as { lefts: number[] }).lefts);
  expect(Math.min(...lefts)).toBeGreaterThan(box.x + 40);
  await expect.poll(styleLeft).toBe(start + 80);
});

test('a channel with presets opens a list and the fader jumps to the chosen one', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();

  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'DMX channels' }).click();
  await page.getByRole('button', { name: 'Patched', exact: true }).click();

  // The color wheel (5th channel of the mode) offers its slots; Pan does not.
  const wheel = page.getByRole('button', { name: /^Presets of channel/ }).first();
  const address = Number(/\d+/.exec((await wheel.getAttribute('aria-label')) ?? '')![0]);
  await expect(page.getByRole('button', { name: `Presets of channel ${address - 4}`, exact: true })).toHaveCount(0);
  await wheel.click();
  await expect(page.getByRole('option', { name: /Red/ })).toBeVisible();
  await page.getByRole('option', { name: /Red/ }).click();

  // "Red" is 10-19: the channel goes to the middle of it, as scenes do.
  await expect(page.getByLabel(`Channel ${address}`, { exact: true })).toHaveValue('14');
  await expect
    .poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels)
    .toContainEqual({ universe: 1, address, value: 14 });
  // The strip names the slot now selected, and the list highlights it.
  await expect(page.locator('.strip.set .ch', { hasText: 'Red' }).first()).toBeVisible();
  await wheel.click();
  await expect(page.getByRole('option', { name: /Red/ })).toHaveAttribute('aria-selected', 'true');
  // Regression: focus went to the first preset instead of the current one.
  await expect(page.getByRole('option', { name: /Red/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox')).toHaveCount(0);

  // Regression: opened upwards (button low in the window), the list covered
  // the button that opened it.
  await page.setViewportSize({ width: 1300, height: 320 });
  await wheel.scrollIntoViewIfNeeded();
  await wheel.click();
  const button = (await wheel.boundingBox())!;
  const list = (await page.getByRole('listbox').boundingBox())!;
  expect(button.y + button.height).toBeGreaterThan(320 * 0.55);
  expect(list.y + list.height).toBeLessThanOrEqual(button.y + 0.5);
  await page.keyboard.press('Escape');
});

test('a color wheel fixture offers its wheel colors instead of a free picker', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await page.getByPlaceholder('Name (default: model)').fill('Wheel spot');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();

  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await page.getByLabel('Scene target').selectOption({ label: 'Wheel spot' });
  // No palette exists yet, so none is offered (it would be an empty list).
  await expect(page.getByLabel('Scene attribute').locator('option')).toHaveText([
    'Intensity', 'Color', 'Position', 'Gobo', 'Prism', 'Zoom', 'Strobe',
  ]);
  await page.getByLabel('Scene attribute').selectOption('color');

  // The slots of the wheel, no free color input.
  const wheel = page.getByRole('radiogroup', { name: 'Color wheel colors' });
  for (const name of ['White', 'Red', 'Green', 'Blue', 'Amber']) {
    await expect(wheel.getByRole('radio', { name, exact: true })).toBeVisible();
  }
  await expect(wheel.getByRole('radio')).toHaveCount(5);
  await expect(page.locator('input[type=color]')).toHaveCount(0);

  await wheel.getByRole('radio', { name: 'Blue', exact: true }).click();
  await expect(wheel.getByRole('radio', { name: 'Blue', exact: true })).toHaveAttribute('aria-checked', 'true');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: 'Color #0000ff' })).toBeVisible();

  // The engine puts the wheel on that slot: Blue is 30-39, so 34.
  const row = page.locator('.card', { hasText: 'Wheel spot' }).locator('tr', { hasText: 'Color Wheel' });
  await expect(row).toContainText('34');

  // Once a palette is stored (Simple Desk), the editor offers it.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Wheel spot', exact: true }).click();
  await page.getByRole('button', { name: 'Full' }).click();
  await page.locator('select').filter({ has: page.locator('option', { hasText: 'Beam' }) }).selectOption('Intensity');
  page.once('dialog', (d) => d.accept('Bright'));
  await page.getByRole('button', { name: 'New palette' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  // Regression: with palettes in the show the editor opened on "Palette".
  await expect(page.getByLabel('Scene attribute')).toHaveValue('intensity');
  await page.getByLabel('Scene target').selectOption({ label: 'Wheel spot' });
  await page.getByLabel('Scene attribute').selectOption('palette');
  await expect(page.locator('select option', { hasText: 'Bright (Intensity)' })).toBeAttached();
});

test('chaser steps are picked in a dialog and can be reordered', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const name = page.locator('.editor input').first();
  for (const scene of ['Step A', 'Step B', 'Step C']) {
    await page.getByRole('button', { name: 'New scene', exact: true }).click();
    await expect(name).toHaveValue(/^Scene \d+$/);
    await name.fill(scene);
    await name.press('Enter');
    await expect(page.getByRole('treeitem', { name: new RegExp(scene) })).toBeVisible();
  }

  // A new chaser opens the dialog; scenes are added in the order of the ticks.
  await page.getByRole('button', { name: 'New chaser', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  // Only scenes can be steps.
  await expect(dialog.getByRole('treeitem', { name: /Chasers/ })).toHaveCount(0);
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByLabel('Select Step A').check();
  // Steps are ordered on the chaser screen only.
  await expect(dialog.getByRole('button', { name: /^Move / })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(dialog).toHaveCount(0);
  const steps = page.getByRole('list', { name: 'Chaser steps' }).locator('li .name');
  await expect(steps).toHaveText(['Step B', 'Step A']);

  // Reopened, it appends: a scene can appear more than once.
  await page.getByRole('button', { name: 'Add scenes…' }).click();
  await dialog.getByLabel('Select Step C').check();
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C', 'Step B']);

  // Cancel changes nothing.
  await page.getByRole('button', { name: 'Add scenes…' }).click();
  await dialog.getByLabel('Select Step A').check();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(steps).toHaveCount(4);

  // Order: arrows, drag and drop, removal.
  await page.getByRole('button', { name: 'Move step 3 up' }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A', 'Step B']);
  await page.getByRole('button', { name: 'Remove step 4' }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);
  await steps.nth(2).dragTo(steps.nth(0));
  await expect(steps).toHaveText(['Step A', 'Step B', 'Step C']);
  await steps.nth(0).dragTo(steps.nth(2));
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);

  // The order is the engine's, not just the page's.
  await page.reload();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);
  await expect(page.getByRole('dialog')).toHaveCount(0);

  // The virtual console still builds a frame with the same dialog.
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.getByRole('button', { name: 'Frame from functions…' }).click();
  await dialog.getByLabel('Solo frame (one at a time)').check();
  await dialog.getByLabel('Frame name').fill('Steps');
  await dialog.getByLabel('Select Step A').check();
  await expect(dialog.getByRole('button', { name: 'Move Step A up' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Create 1 button' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('Steps (solo)', { exact: true })).toBeVisible();
});
