#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# vim: set sw=4 expandtab:
#
# Created: 2018-06-21 11:05:30+02:00
# Main authors:
#     - Jérôme Pouiller <jerome.pouiller@silabs.com>
#
# Copyright (c) 2018, Silicon Laboratories
# See license terms contained in COPYING file
#

# Generate a compressed version of PDS from a full PDS.
#
# Differences between full PDS and compressed PDS are:
#     1. Comments are stripped
#     2. Symbolic names are replaced with their values
#     3. Hexadecimal and binary numbers are replaced by their decimal values
#     4. New lines and spaces are stripped
#
# In two words, output of this script is more or less same as:
#     cpp -undef -P | tr -d '\n\t '
#
#
# Most C Preprocessor directives are not supported:
#    - #if (however, #ifdef family is supported)
#    - Multiple lines defines
#    - Recursive defines
#    - Support for strings (defines in strings, comment in strings, etc...)

# Avoid syntax error with python 2.x
from __future__ import print_function

# If you modifiy this file, please don't forget to increment version number.
__version__ = "1.1"

import io
import os
import sys
import re
import textwrap
import argparse

class DebugInfo():
    def __init__(self, path = "", line = 0):
        self.path = path
        self.line = line

class AnnotOut():
    def __init__(self, loc, val):
        self.loc = DebugInfo(loc.path, loc.line)
        self.val = val

# Definitions declared by user using #define
g_defs = { }
# Result. Use of arrays instead of a string is a trick to avoid declaring it global in each function
g_result = [ ]
# Value returned by script
g_ret_value = 0

def pr_info(dbg_info, message):
    global g_ret_value
    g_ret_value = 1
    print("%s:%d:%s" % (dbg_info.path, dbg_info.line, message), file=sys.stderr)

def add_define(dbg_info, var, val):
    if var in g_defs:
        pr_info(dbg_info, 'warning: "%s" redefined' % var)
    if val in g_defs:
        val = g_defs[val]
    val = re.sub(r'^"', '', val)
    val = re.sub(r'"$', '', val)
    if len(val) > 1 and not re.match(r'^-?(0[xb])?[a-fA-F0-9_]+$', val):
        pr_info(dbg_info, 'warning: dictionary entry "%s" does not contains a valid token: %s' % (var, val))
    g_defs[var] = val


def handle_ifdef_stack(dbg_info, line, stack):
    if re.match(r'\s*#\s*ifn?def\s+', line, re.I):
        m = re.match(r'^\s*#\s*if(n?)def\s+([a-zA-Z_]\w*)\s*$', line, re.I)
        if not m:
            pr_info(dbg_info, 'error: bad #ifdef directive (%s)'% line)
        else:
            if bool(m.group(2) in g_defs) ^ bool(m.group(1) == "n"):
                stack.append(stack[-1])
            else:
                stack.append(False)
        return ""
    if re.match(r'\s*#\s*else', line, re.I):
        m = re.match(r'^\s*#\s*else\s*$', line, re.I)
        if not m:
            pr_info(dbg_info, 'error: text after #else directive')
        else:
            if len(stack) < 2:
                pr_info(dbg_info, 'error: unbalanced #else')
            if stack[-2]:
                stack[-1] = not stack[-1]
            elif stack[-1]:
                pr_info(dbg_info, "internal error")
        return ""
    if re.match(r'\s*#\s*endif', line, re.I):
        m = re.match(r'^\s*#\s*endif\s*$', line, re.I)
        if not m:
            pr_info(dbg_info, 'error: text after #endif directive')
        else:
            stack.pop()
            if len(stack) < 1:
                pr_info(dbg_info, 'error: unbalanced #endif')
        return ""
    if not stack[-1]:
        return ""
    return line

def handle_define(dbg_info, line):
    if re.match(r'\s*#\s*define\s+', line, re.I):
        m = re.match(r'^\s*#\s*define\s+([a-zA-Z_]\w*)(\s+(.*))?$', line, re.I)
        if not m:
            pr_info(dbg_info, 'error: bad #define directive')
        else:
            if m.lastindex > 1:
                val = m.group(3)
            else:
                val = ""
            add_define(dbg_info, m.group(1), val)
            return ""
    return line

def handle_include(dbg_info, line, inc_paths):
    if re.match(r'\s*#\s*include\s+', line, re.I):
        m = re.match(r'^\s*#\s*include\s+"([\w\./-]+)"\s*$', line, re.I)
        if not m:
            pr_info(dbg_info, 'error: bad #include directive')
            exit(g_ret_value)
        else:
            file_list = (os.path.join(dir, m.group(1)) for dir in inc_paths)
            try:
                file = next(f for f in file_list if os.path.isfile(f))
            except StopIteration:
                pr_info(dbg_info, 'error: cannot find file "%s"' % m.group(1))
                exit(g_ret_value)
            with open(file) as f_inc:
                dbg_info_inc = DebugInfo(m.group(1))
                new_inc_paths = inc_paths[:-1]
                new_inc_paths.append(os.path.dirname(file))
                parse(dbg_info_inc, f_inc, new_inc_paths)
        return ""
    return line

def replace_definitions(dbg_info, line):
    out = ""
    ptr = 0
    for m in re.finditer(r'["\w]+', line):
        word = m.group(0)
        word = re.sub(r'^"', '', word)
        word = re.sub(r'"$', '', word)
        if not word in g_defs:
            if len(word) > 1 and not re.match(r'^-?(0[xb])?[a-fA-F0-9_]+$', word):
                pr_info(dbg_info, "error: %s was not found in dictionary" % word)
        else:
            word = g_defs[word]
            if len(word) > 1 and not re.match(r'^-?(0[xb])?[a-fA-F0-9_]+$', word):
                pr_info(dbg_info, "error: %s is not a valid token" % word)
        out += line[ptr:m.start(0)]
        out += word
        ptr = m.end(0)
    out += line[ptr:]
    return out

def replace_numbers(dbg_info, line):
    def convert(n, base):
        return str(int(re.sub(r'_', '', n), base))
    def convert_hex(m):
        return convert(m.group(0)[2:], 16)
    def convert_bin(m):
        return convert(m.group(0)[2:], 2)
    def convert_dec(m):
        return "%X" % int(convert(m.group(0), 10))

    # Convert all numbers in decimal before converting to hexadecimal
    line = re.sub(r'0x[0-9a-fA-F_]+', convert_hex, line)
    line = re.sub(r'0b[0-1_]+', convert_bin, line)
    line = re.sub(r'[0-9_]+', convert_dec, line)
    return line

def fix_outermost_braces(dbg_info):
    global g_result
    if not next(token_iter()).val in [ '[', '{' ]:
        new_token = AnnotOut(DebugInfo(dbg_info.path, 0), '{')
        g_result.insert(0, new_token)
        new_token = AnnotOut(DebugInfo(dbg_info.path, dbg_info.line), '}')
        g_result.append(new_token)

def parse(dbg_info, f_in, inc_paths):
    ifdef_stack = [ True ]
    multiline_comment = False;
    for line in f_in:
        dbg_info.line += 1
        line = line.strip()
        if multiline_comment:
            (line, n) = re.subn(r'.*?\*/', "", line, 1);
            if n != 0:
                multiline_comment = False
            else:
                line = ""
        if not multiline_comment:
            line = re.sub(r'//.*', "", line);
            line = re.sub(r'/\*.*?\*/', " ", line) # Note the space inside "
            line, n = re.subn(r'/\*.*', "", line);
            if n != 0:
                if n != 1:
                    pr_info(dbg_info, "internal error")
                multiline_comment = True
            line = line.strip()
            line = handle_ifdef_stack(dbg_info, line, ifdef_stack)
            line = handle_include(dbg_info, line, inc_paths)
            line = handle_define(dbg_info, line)
            line = replace_definitions(dbg_info, line)
            line = replace_numbers(dbg_info, line)
        line = re.sub(r'\s', '', line);
        g_result.append(AnnotOut(dbg_info, line))
    if multiline_comment:
        pr_info(dbg_info, "error: unfinished comment")
    if len(ifdef_stack) > 1:
        pr_info(dbg_info, "error: unbalanced #ifdef")
    # Further processes need a non-empty result to retrieve dbg_info
    if len(g_result) == 0:
        g_result.append(AnnotOut(dbg_info, ""))

def token_iter():
    for line in g_result:
        for char in line.val:
            # FIXME: A to F are valid identifiers, however, we always consider
            # them as numbers
            if re.match(r'[-0-9A-F]', char):
                yield AnnotOut(line.loc, 'd')
            elif re.match(r'\w', char):
                yield AnnotOut(line.loc, 'w')
            else:
                yield AnnotOut(line.loc, char)
    yield AnnotOut(g_result[-1].loc, 'E')

def check_syntax():
    # There are small approximations in this state machine. For example, a:a:a is
    # accepted.
    # Symbolic character list:
    #    'S' Start
    #    'E' End
    #    'w' Single letter
    #    'd' Single digit
    # Also notice this table allows ',' to be followed by '}', ']' or E. In
    # final string, it won't be the case, but here, we haven't done this
    # simplification yet
    states = {
        'S': [ '{', '[' ],
        'w': [ '}', ']', ':', ',' ],
        'd': [ '}', ']', ',', 'd' ],
        ':': [ '{', '[', 'w', 'd' ],
        ',': [ '{', '[', '}', ']', 'w', 'd', 'E' ],
        '{': [ '{', '[', '}', ']', 'w' ],
        '[': [ '{', '[', '}', ']', 'w', 'd'],
        '}': [ ',', ']', '}', 'E' ],
        ']': [ ',', ']', '}', 'E' ],
    }
    brace_stack = [ ];
    cur_state = AnnotOut(g_result[-1].loc, 'S');

    for annot_tok in token_iter():
        tok = annot_tok.val
        if not tok in states[cur_state.val]:
            # pr_info(cur_state.loc, "error: parsing error (state '%s' cannot be followed by '%s')" % (cur_state.val, tok));
            if cur_state.val in [ 'w' ] and tok in [ '{', '[', 'd' ]:
                pr_info(cur_state.loc, "error: parsing error (missing colon?)")
            elif cur_state.val in [ 'd', 'w', '}', ']' ] and tok in [ 'd', 'w' ]:
                pr_info(cur_state.loc, "error: parsing error (missing comma?)")
            else:
                pr_info(annot_tok.loc, "error: parsing error")
            return False
        else:
           cur_state = annot_tok
           if tok in [ '{', '[' ]:
               brace_stack.append(tok)
           if tok == '}':
               if brace_stack.pop() != '{':
                   pr_info(annot_tok.loc, "error: unexpected '}'");
                   return False
           if tok == ']':
               if brace_stack.pop() != '[':
                   pr_info(annot_tok.loc, "error: unexpected ']'");
                   return False
           if tok == 'E' and len(brace_stack) > 0:
               pr_info(annot_tok.loc, "error: unbalanced %s" % brace_stack.pop());
               return False
    return True

def check_sizes(pds_str):
    brace_level = 0
    num_token = 0
    num_char = 0
    num_top_node = 1
    for c in pds_str:
        if c in ",:}]":
            num_token += 1
        if c == '{' or c == '[':
            brace_level += 1
        if c == '}' or c == ']':
            brace_level -= 1
            if brace_level == 1:
                if num_token >= 256:
                    print("warning: too much tokens in top-node %d (%d nodes)"
                            % (num_top_node, num_token), file=sys.stderr)
                    g_ret_value = 1
                if num_char >= 1499:
                    print("warning: top-node %d is too large (%d bytes)"
                            % (num_top_node, num_char), file=sys.stderr)
                    g_ret_value = 1
                num_token = 0
                num_char = 0
                num_top_node += 1
    if brace_level != 0:
        print("error: internal error (please report) %d" % brace_stack, file=sys.stderr)

tmpl_c = """\
    /* AUTOMATICALLY GENERATED -- DO NOT EDIT BY HAND */
    /*
     * Copyright 2018, Silicon Laboratories Inc.  All rights reserved.
     *
     * Licensed under the Apache License, Version 2.0 (the "License");
     * you may not use this file except in compliance with the License.
     * You may obtain a copy of the License at
     *
     *     http://www.apache.org/licenses/LICENSE-2.0
     *
     * Unless required by applicable law or agreed to in writing, software
     * distributed under the License is distributed on an "AS IS" BASIS,
     * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
     * See the License for the specific language governing permissions and
     * limitations under the License.
     *
     */

    /**
     * \\file wf200_pds.h
     * \\brief contains the PDS configuration specific to a hardware configuration.
     */

    #ifndef WF200_PDS_H
    #define WF200_PDS_H

    static const char* const wf200_pds[] = {
    %s};

    #endif
    """

def formatc(f_out, pds):
    stack = 0
    buf = ""
    out = ""
    for c in pds:
        buf += c
        if c == '{' or c == '[':
            stack += 1
        if c == '}' or c == ']':
            stack -= 1
        if (c == '}' or c == ']') and stack == 1:
            out += '    "{%s}",\n' % buf[1:];
            buf = ""
    f_out.write(textwrap.dedent(tmpl_c) % out);

tmpl_rust = """\
    /* AUTOMATICALLY GENERATED -- DO NOT EDIT BY HAND */
    /*
     * Copyright 2018, Silicon Laboratories Inc.  All rights reserved.
     *
     * Licensed under the Apache License, Version 2.0 (the "License");
     * you may not use this file except in compliance with the License.
     * You may obtain a copy of the License at
     *
     *     http://www.apache.org/licenses/LICENSE-2.0
     *
     * Unless required by applicable law or agreed to in writing, software
     * distributed under the License is distributed on an "AS IS" BASIS,
     * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
     * See the License for the specific language governing permissions and
     * limitations under the License.
     *
     */

/// PDS (platform data set) is SiLab's solution to configuring the wifi chip.
/// Options such as GPIO, PA power, number of antennae are configured with PDS.
/// The data set here is "compiled" using a script (pds_compress) that
/// can be found in SiliconLabs github repository:
/// https://github.com/SiliconLabs/wfx-linux-tools/blob/master/pds_compress
/// 
/// The output is meant to be redirected into a .rs file in the same crate as your
/// hal driver, and pulled in with the appropriate mod/use statements.

pub const PDS_DATA: [&[u8]; %d] = [
%s];
"""

def formatrust(f_out, pds):
    stack = 0
    lines = 0
    buf = ""
    out = ""
    for c in pds:
        buf += c
        if c == '{' or c == '[':
            stack += 1
        if c == '}' or c == ']':
            stack -= 1
        if (c == '}' or c == ']') and stack == 1:
            lines += 1
            out += '    b"{%s}\\0", \n' % buf[1:];
            buf = ""
    f_out.write(textwrap.dedent(tmpl_rust) % (lines, out));

def formattiny(f_out, pds):
    stack = 0
    for c in pds:
        if c == '}' or c == ']':
            stack -= 1
            f_out.write("\n" + "    " * stack);
        f_out.write(c);
        if c == ':':
            f_out.write(" ");
        if c == '{' or c == '[':
            stack += 1
            f_out.write("\n" + "    " * stack);
        if c == ',':
            f_out.write("\n" + "    " * stack);
    f_out.write("\n");

def parse_cmdline(args=sys.argv[1:]):
    parser = argparse.ArgumentParser(usage="%(prog)s [options] INPUT [OUTPUT]",
                  description="Generate a compressed version of PDS from a full PDS")
    parser.add_argument('--version', action='version',
                  version='%(prog)s {version}'.format(version=__version__))
    parser.add_argument("input", metavar='INPUT', type=argparse.FileType('r'),
                  help="input file (except C format, all output formats can be used as input)")
    parser.add_argument("output", metavar='OUTPUT', nargs="?", type=argparse.FileType('w'), default=sys.stdout,
                  help="output file (standard output if not specified)")
    parser.add_argument("-I", "--include", action='append', dest="includes", metavar="DIR",
                  help="search includes in this subdirectory")
    parser.add_argument("-D", "--define", action='append', dest="defines", metavar="DEF[=VAL]",
                  help="predefine DEF with value VAL")
    parser.add_argument("-f", "--force", action="store_true", dest="force",
                  help="try to produce (probably broken) output even if errors are detected")
    parser.add_argument("--out", dest="out_format", default="pds", choices=['pds', 'tinypds', 'c', 'json'],
                  help="specify output format. Accepted values: pds (compressed PDS), tinypds (indented PDS), c (C file), json. Default: pds")
    parser.add_argument("-j", action="store_const", const="json", dest="out_format",
                  help="shortcut for --out=json")
    parser.add_argument("-c", action="store_const", const="c", dest="out_format",
                  help="shortcut for --out=c")
    parser.add_argument("-p", action="store_const", const="pds", dest="out_format",
                  help="shortcut for --out=pds")
    parser.add_argument("-t", action="store_const", const="tinypds", dest="out_format",
                  help="shortcut for --out=tinypds")
    parser.add_argument("-r", action="store_const", const="rust", dest="out_format",
                  help="shortcut for --out=rust")
    return parser.parse_args(args)

def main(options):
    global g_ret_value

    for d in options.defines or [ ]:
        if "=" in d:
            (var, val) = d.split('=', 1)
        else:
            (var, val) = (d, "")
        add_define(DebugInfo("<cmdline>"), var, val)
    inc_paths = [ ]
    if options.includes:
        inc_paths = options.includes + inc_paths
    if options.input == sys.stdin or not hasattr(options.input, 'name'):
        inc_paths += [ "." ]
    else:
        inc_paths += [ os.path.dirname(options.input.name) ]
    if not hasattr(options.input, 'name'):
        dbg_info = DebugInfo("<inline>")
    else:
        dbg_info = DebugInfo(options.input.name)
    parse(dbg_info, options.input, inc_paths)
    if g_ret_value and not options.force:
        return g_ret_value
    fix_outermost_braces(dbg_info)
    check_syntax()
    if g_ret_value and not options.force:
        return g_ret_value
    str_result = ''.join(x.val for x in g_result)
    str_result = re.sub(r',\]', ']', str_result)
    str_result = re.sub(r',}', '}', str_result)
    check_sizes(str_result)
    if g_ret_value and not options.force:
        return g_ret_value
    if options.out_format == "json":
        formattiny(options.output, re.sub(r'([-A-Za-z0-9]+)', r'"\1"', str_result))
    elif options.out_format == "c":
        formatc(options.output, str_result)
    elif options.out_format == "rust":
        formatrust(options.output, str_result)
    elif options.out_format == "tinypds":
        formattiny(options.output, str_result)
    elif options.out_format == "pds":
        options.output.write(str_result)
    else:
        raise Exception('bad out_format value')
    return g_ret_value

# This function is only an help for third-party tools that import pds_compress
# as a python module (also note it is necessary to add .py extention to this in
# order to import it).
def compress_string(str_in, extra_options=""):
    options = parse_cmdline([ "-" ] + extra_options.split())
    options.input = io.StringIO(str_in)
    options.output = io.StringIO()
    main(options)
    return options.output.getvalue()

if __name__ == '__main__':
    if sys.version_info < (3, 0):
        sys.stderr.write("This tools was developed for Python 3 and wasn't tested with Python 2.x\n")
    options = parse_cmdline()
    sys.exit(main(options))
