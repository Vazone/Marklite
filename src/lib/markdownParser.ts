import { GFM, parser } from '@lezer/markdown';
import { markdownInline } from './markdownInline';
import { markdownMetadata } from './markdownMetadata';
import { markNodes, markStyles, marksWrapper } from './markdownMarks';

// The worker's packed node IDs must use the same extension order as CodeMirror.
const base = parser.configure([GFM, markdownInline, markdownMetadata, { defineNodes: markNodes, props: [markStyles] }]);
export const markdownParser = base.configure({ wrap: marksWrapper(base.nodeSet) });
