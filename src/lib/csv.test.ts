import { describe, expect, it } from 'vitest';

import { csvField, csvRow, csvText } from './csv';

describe('csv', () => {
  it('mock_csv_neutralises_formulas', () => {
    expect(csvField('=HYPERLINK("http://x")')).toBe(
      '"\'=HYPERLINK(""http://x"")"',
    );
    expect(csvField('+1')).toBe("'+1");
    expect(csvField('-cmd')).toBe("'-cmd");
    expect(csvField('@SUM(A1)')).toBe("'@SUM(A1)");
    expect(csvField('\tTab')).toBe("'\tTab");
    expect(csvField('Mega Man X')).toBe('Mega Man X');
  });

  it('quotes only when needed', () => {
    expect(csvField('Tom, Jerry')).toBe('"Tom, Jerry"');
    expect(csvField('two\nlines')).toBe('"two\nlines"');
    expect(csvField(' padded')).toBe('" padded"');
    expect(csvRow(['a', 1, '<img src=x>'])).toBe('a,1,<img src=x>');
    expect(
      csvText([
        ['a', 'b'],
        [1, '=x'],
      ]),
    ).toBe("a,b\n1,'=x\n");
  });
});
