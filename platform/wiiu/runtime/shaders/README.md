# Shaders

`textureShader` draws a textured quad tinted by its vertex color: `a_position` (vec2) through the
`u_projection` matrix, `a_color` (vec4), `a_texcoord` (vec2); the pixel is the texture times the
color. `textureShader.vsh` and `textureShader.psh` are its Latte assembly, and
`textureShader.inc` the compiled shader group (a GFD file, from decaf-emu's `latte-assembler`
and `xxd -i`), which `runtime/main.c` includes.

They come unchanged from the Wii U renderer of devkitPro's SDL port
(<https://github.com/devkitPro/SDL>, branch `wiiu-sdl2-2.28`, `src/render/wiiu/shaders`), under
SDL's zlib license:

> Simple DirectMedia Layer
> Copyright (C) 2022 GaryOderNichts <garyodernichts@gmail.com>
>
> This software is provided 'as-is', without any express or implied warranty. In no event will
> the authors be held liable for any damages arising from the use of this software.
>
> Permission is granted to anyone to use this software for any purpose, including commercial
> applications, and to alter it and redistribute it freely, subject to the following
> restrictions:
>
> 1. The origin of this software must not be misrepresented; you must not claim that you wrote
>    the original software. If you use this software in a product, an acknowledgment in the
>    product documentation would be appreciated but is not required.
> 2. Altered source versions must be plainly marked as such, and must not be misrepresented as
>    being the original software.
> 3. This notice may not be removed or altered from any source distribution.
