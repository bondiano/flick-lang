// Seeded ISAAC generator, adapted from effect@4.1.2 Random.ts.
import { item } from "../internal.ts";
/*///////////////////////////////////////////////////////////////////////////////////////////////////
This is a derivative work copyright (c) 2025 Effectful Technologies Inc, under MIT license.
This is a derivative work copyright (c) 2018, William P. "Mac" McMeans, under BSD license.
Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of isaacCSPRNG nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.
THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
Original work copyright (c) 2012 Yves-Marie K. Rinquin, under MIT license.
https://github.com/rubycon/isaac.js
///////////////////////////////////////////////////////////////////////////////////////////////////*/
function ISAAC_CSPRNG(inputSeed: string | number) {
  // Internal State
  const memory: number[] = Array.from({ length: 256 }, () => 0)
  const result: number[] = Array.from({ length: 256 }, () => 0)
  let accumulator = 0
  let lastResult = 0
  let generation = 0
  let counter = 0

  // Initial Seed
  seedState(inputSeed)

  function reset() {
    accumulator = 0
    lastResult = 0
    counter = 0
    for (let i = 0; i < 256; ++i) {
      memory[i] = 0
      result[i] = 0
    }
    generation = 0
  }

  function seedState(seedInput: string | number): void {
    // The golden ratio ( 2654435769 )
    // See https://stackoverflow.com/questions/4948780/magic-number-in-boosthash-combine
    const magicNumber = 0x9e3779b9
    let a = magicNumber
    let b = magicNumber
    let c = magicNumber
    let d = magicNumber
    let e = magicNumber
    let f = magicNumber
    let g = magicNumber
    let h = magicNumber
    let i = 0

    let seedWords: Array<number>
    if (typeof seedInput === "string") {
      seedWords = toIntArray(seedInput)
    } else {
      seedWords = [seedInput]
    }

    reset()

    for (i = 0; i < seedWords.length; i++) {
      result[i & 0xff] = item(result, i & 0xff) + item(seedWords, i)
    }

    function mix() {
      a ^= b << 11
      d = add32(d, a)
      b = add32(b, c)

      b ^= c >>> 2
      e = add32(e, b)
      c = add32(c, d)

      c ^= d << 8
      f = add32(f, c)
      d = add32(d, e)

      d ^= e >>> 16
      g = add32(g, d)
      e = add32(e, f)

      e ^= f << 10
      h = add32(h, e)
      f = add32(f, g)

      f ^= g >>> 4
      a = add32(a, f)
      g = add32(g, h)

      g ^= h << 8
      b = add32(b, g)
      h = add32(h, a)

      h ^= a >>> 9
      c = add32(c, h)
      a = add32(a, b)
    }

    // Scramble the seed
    for (i = 0; i < 4; i++) {
      mix()
    }

    for (i = 0; i < 256; i += 8) {
      // Use all the information in the seed
      a = add32(a, item(result, i))
      b = add32(b, item(result, i + 1))
      c = add32(c, item(result, i + 2))
      d = add32(d, item(result, i + 3))
      e = add32(e, item(result, i + 4))
      f = add32(f, item(result, i + 5))
      g = add32(g, item(result, i + 6))
      h = add32(h, item(result, i + 7))

      mix()

      // Fill in the memory with messy stuff
      memory[i] = a
      memory[i + 1] = b
      memory[i + 2] = c
      memory[i + 3] = d
      memory[i + 4] = e
      memory[i + 5] = f
      memory[i + 6] = g
      memory[i + 7] = h
    }

    // Second pass to make sure seed affects memory
    for (i = 0; i < 256; i += 8) {
      a = add32(a, item(memory, i))
      b = add32(b, item(memory, i + 1))
      c = add32(c, item(memory, i + 2))
      d = add32(d, item(memory, i + 3))
      e = add32(e, item(memory, i + 4))
      f = add32(f, item(memory, i + 5))
      g = add32(g, item(memory, i + 6))
      h = add32(h, item(memory, i + 7))

      mix()

      // Fill in the memory with messy stuff (again)
      memory[i] = a
      memory[i + 1] = b
      memory[i + 2] = c
      memory[i + 3] = d
      memory[i + 4] = e
      memory[i + 5] = f
      memory[i + 6] = g
      memory[i + 7] = h
    }

    pnrg()

    generation = 256
  }

  function pnrg(n?: number): void {
    let i = 0
    let x = 0
    let y = 0

    n = n === undefined ? 1 : Math.abs(Math.floor(n))

    while (n--) {
      counter = add32(counter, 1)
      lastResult = add32(lastResult, counter)

      for (i = 0; i < 256; i++) {
        switch (i & 3) {
          case 0: {
            accumulator ^= accumulator << 13
            break
          }
          case 1: {
            accumulator ^= accumulator >>> 6
            break
          }
          case 2: {
            accumulator ^= accumulator << 2
            break
          }
          case 3: {
            accumulator ^= accumulator >>> 16
            break
          }
        }

        accumulator = add32(item(memory, (i + 128) & 0xff), accumulator)
        x = item(memory, i)

        memory[i] = add32(item(memory, (x >>> 2) & 0xff), add32(accumulator, lastResult))
        y = item(memory, i)

        result[i] = add32(item(memory, (y >>> 10) & 0xff), x)
        lastResult = item(result, i)
      }
    }
  }

  /**
   * Returns a signed, random integer in the range [-2^31, 2^31).
   */
  function nextInt32(): number {
    if (!generation--) {
      pnrg()
      generation = 255
    }
    return item(result, generation)
  }

  function nextIntUnsafe(): number {
    return Math.floor(nextDoubleUnsafe() * (Number.MAX_SAFE_INTEGER - Number.MIN_SAFE_INTEGER + 1)) +
      Number.MIN_SAFE_INTEGER
  }

  /**
   * Returns a 53-bit fraction in the range [0, 1).
   */
  function nextDoubleUnsafe(): number {
    const hi = (nextInt32() >>> 0) & 0x1FFFFF // take top 21 bits
    const lo = nextInt32() >>> 0 // full 32 bits

    // 53-bit integer
    const combined = hi * 4294967296 + lo

    return combined / 0x20000000000000 // [0, 1)
  }

  return { nextIntUnsafe, nextDoubleUnsafe }
}

/**
 * 32-bit addition with overflow handling (JavaScript numbers are 53-bit).
 *
 * Example: add32(0xFFFFFFFF, 0x00000001) = 0x00000000 (wraps around)
 */
function add32(x: number, y: number): number {
  // Add lower 16 bits separately to handle carry
  // Example: x=0x12345678, y=0xABCDEF01
  // lsb = (0x5678 + 0xEF01) = 0x14579
  const lsb = (x & 0xffff) + (y & 0xffff)

  // Add upper 16 bits + carry from lower addition
  // msb = (0x1234 + 0xABCD + (0x14579 >>> 16)) = (0x1234 + 0xABCD + 0x1) = 0xBE02
  const msb = (x >>> 16) + (y >>> 16) + (lsb >>> 16)

  // Combine: upper 16 bits | lower 16 bits (masked to prevent double carry)
  // return (0xBE02 << 16) | (0x14579 & 0xffff) = 0xBE024579
  return (msb << 16) | (lsb & 0xffff)
}

const seedEncoder = new TextEncoder()

/**
 * Convert a string to UTF-8 encoded 32-bit integers (little-endian).
 */
function toIntArray(seed: string): Array<number> {
  const bytes = seedEncoder.encode(seed)
  const result: Array<number> = []

  for (let index = 0; index < bytes.length; index += 4) {
    result.push(
      ((bytes[index] ?? 0) << 0) |
        ((bytes[index + 1] ?? 0) << 8) |
        ((bytes[index + 2] ?? 0) << 16) |
        ((bytes[index + 3] ?? 0) << 24)
    )
  }

  return result
}

export const isaac = ISAAC_CSPRNG;
