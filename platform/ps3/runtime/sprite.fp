!!ARBfp1.0
# The texel times the vertex color. No named temporaries: cgcomp's assembler gives a TEMP one
# register when it is written and another when it is read, so the texel goes straight into the
# output (r0) and is multiplied there.
TEX result.color, fragment.texcoord[0], texture[0], 2D;
MUL result.color, result.color, fragment.color;
END
