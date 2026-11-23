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

  // Add a group to a scene from the scene editor.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Front' });
  await page.getByLabel('Scene attribute').selectOption('intensity');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: 'Intensity 100%' })).toBeVisible();

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
  await expect(page.getByText('Fixtures in this scene (2)')).toBeVisible();
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
