#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""A simple tool to do whatever it takes to add error-correcting and reundancy
to backup files in order to not waste space on a write-once archival medium.

--snip--

(Intended for when I want to back up floppy disks and then put a copy of the
 backups in the game box.)

This script makes use of the following tools:
- genisoimage (For creating an ISO from the provided files so that redundancies
               like "original ISO filesystem structures PLUS UDF structures"
               can be enabled and ECC can be applied at a lower level.
- dvdisaster (For augmenting the ISO with ECC padding that protects even the
              filesystem structures and low-level bitstream, but it's not
              enough on its own because it will not inflate the ISO to more
              than three times its original size and no big-box game came on
              hundreds of megabytes worth of floppies.)
- par2create (For adding a layer of ECC within the filesystem so there's more
              to be multiplied by three to utilize as much of the filesystem as
              possible.)
"""

from __future__ import (absolute_import, division, print_function,
                        with_statement, unicode_literals)

__author__ = "Stephan Sokolow (deitarion/SSokolow)"
__appname__ = "CD Ballooner"
__version__ = "0.0pre0"
__license__ = "MIT OR Apache-2.0"

import logging, multiprocessing, os, shlex, shutil, sys, tempfile
import subprocess  # nosec
from argparse import ArgumentParser, RawDescriptionHelpFormatter
from collections import OrderedDict
log = logging.getLogger(__name__)

GENISOFS_OPTS = [
    '-appid', __appname__,
    '-sysid', "LINUX",   # TODO: Don't hard-code this
    '-quiet',            # TODO: Filter the output to convert progress \n to \r
    '-no-cache-inodes',  # err on the side of caution
    '-udf',              # Write a UDF TOC in parallel to ISO for the data
    '-iso-level', '1',   # DOS-compatible filesystem format
    '-joliet',           # Use Joliet for DOS-compatible Win9x LFN (-J)
    '-rational-rock',    # Use Rock Ridge for authoritative filenames (-r)
    '-translation-table',  # Show authoritative names to DOS via TRANS.TBL (-T)
    '-hide-joliet-trans-tbl',  # Avoid clutter in the Joliet tree
]

# TODO: Support dynamically reducing redundancy (but not below default) to
# allow another iteration of `cat temp.iso temp.iso > final.iso` to gain more
# redundancy for the filesystem structures and DVDisaster metadata.
PAR2_CMD = ['par2', 'c', '-n1', '-r20']

ARCHIVERS = OrderedDict([  # Sorted in priority order
    ('.zip', 'zip -rT'),
    ('.tar', 'tar cf'),
    ('.7z', '7z a -y'),
    ('.rar', 'rar a -r -rr -t -y'),
    ('.lzh', 'jlha a'),
    ('.arj', 'arj a -r -hk -y'),
    ('.zoo', 'zoo ah'),
])

COMPRESSORS = OrderedDict([  # Sorted in priority order
    ('.gz', 'gzip -k'),
    ('.bz2', 'bzip2 -k'),
    ('.lz', 'lzip -k'),
    ('.lzma', 'lzma -k'),
    ('.xz', 'xz -k'),
    # ('.Z', 'compress'),  # TODO: Needs to fake -k
])

EXTENSION_COMPRESSION = {
    '.tar.bz2': '.tbz2',
    '.tar.gz': '.tgz',
    '.tar.lz': '.tlz',
    '.tar.xz': '.txz',
    # '.tar.Z': '.taZ',
}


def copy(src, dest):
    """Copy a file or directory without caring which the source is."""
    if os.path.isdir(src):
        shutil.copytree(src, dest)
    else:
        shutil.copy(src, dest)


def escape_graft(path):
    """Escape an unescaped path for use with a -graft-points

    WARNING: Must be applied before adding the significant '='
    """
    return path.replace('\\', '\\\\').replace('=', '\\=')


def parchive(src_path):
    """Generate a maximum-redundancy .par2 archive for the source path

    (It will be placed in the same parent directory)"""
    par_dir, src_name = os.path.split(src_path)

    if os.path.isdir(src_path):
        files = []
        for parent, _, fnames in os.walk(src_path):
            files.extend(os.path.join(parent, fname) for fname in fnames)
        files = [os.path.relpath(path, par_dir) for path in files]
        subprocess.check_call(PAR2_CMD + [src_path + '.par2'] + files,  # nosec
                              cwd=par_dir)
    else:
        subprocess.check_call(  # nosec
            PAR2_CMD + [src_path + '.par2', src_name], cwd=par_dir)

# TODO: Redesign this so it does format-by-format iteration so that, if there's
# enough data to actually fill the disc with the help of this process, there
# won't be an unequal distribution of redundancy.


def process(inpath, outdir):
    """Top-level entry point for per-command-line-argument operations"""
    outdir = os.path.abspath(outdir)
    outname = os.path.basename(inpath)
    outpath = os.path.join(outdir, outname)
    log.info("Processing %r -> %r", inpath, outdir)

    log.info("Copying %r -> %r", inpath, outpath)
    copy(inpath, outpath)

    for ext, archiver in ARCHIVERS.items():
        archive_path = outpath + ext
        if os.path.exists(archive_path):
            log.info("Skipping. Already exists: %s", archive_path)
        else:
            log.info("Archiving %r -> %r", inpath, archive_path)

        # TODO: Handle nonzero return codes and missing commands
        argv = shlex.split(archiver) + [archive_path, os.path.basename(inpath)]
        subprocess.check_call(argv, cwd=outdir)  # nosec

    for ext, compressor in COMPRESSORS.items():
        if os.path.isfile(outpath):
            log.info("Compressing %r with %r", inpath, compressor)
            argv = shlex.split(compressor) + [outpath]
            subprocess.check_call(argv, cwd=outdir)  # nosec

        out_tar = outname + '.tar'
        log.info("Compressing %r with %r", out_tar, compressor)
        argv = shlex.split(compressor) + [out_tar]
        subprocess.check_call(argv, cwd=outdir)  # nosec

    for ext_from, ext_to in EXTENSION_COMPRESSION.items():
        src = outpath + ext_from
        dest = outpath + ext_to
        os.rename(src, dest)


def generate_iso(src_dir, outpath, volume_id):
    """Generate a DVDisaster-augmented ISO from the given folder"""
    src_dir = os.path.abspath(src_dir)

    grafts_seen, grafts = [], []
    for fname in os.listdir(src_dir):
        name = escape_graft(fname)
        path = escape_graft(os.path.join(src_dir, fname))

        if name in grafts_seen:
            raise AssertionError("Naming collision: {}".format(name))
        grafts_seen.append(name)
        grafts.append(name + '=' + path)

    subprocess.check_call(['genisoimage'] + GENISOFS_OPTS +  # noqa # nosec
        ['-volid', volume_id, '-o', outpath, '-graft-points'] + grafts)

    cores = multiprocessing.cpu_count()
    subprocess.check_call(['dvdisaster', '-c', '-x', str(cores),  # nosec
        '-mRS02', '-n', 'CD', '-i', outpath])


def main():
    """The main entry point, compatible with setuptools entry points."""
    # If we're running on Python 2, take responsibility for preventing
    # output from causing UnicodeEncodeErrors. (Done here so it should only
    # happen when not being imported by some other program.)
    if sys.version_info.major < 3:
        reload(sys)  # noqa # pylint: disable=undefined-variable
        sys.setdefaultencoding('utf-8')  # pylint: disable=no-member

    parser = ArgumentParser(formatter_class=RawDescriptionHelpFormatter,
            description=__doc__.replace('\r\n', '\n').split('\n--snip--\n')[0])
    parser.add_argument('--version', action='version',
            version="%%(prog)s v%s" % __version__)
    parser.add_argument('-v', '--verbose', action="count",
        default=2, help="Increase the verbosity. Use twice for extra effect.")
    parser.add_argument('-q', '--quiet', action="count",
        default=0, help="Decrease the verbosity. Use twice for extra effect.")
    parser.add_argument('inpath', nargs='+', help='Files/folders to copy into '
                        'the root of the ECC-protected ISO and protect.')
    parser.add_argument('--volid', default=None,
        help="Specify the volume ID for the generated ISO file. (Default: "
        "the first 32 characters of the first file's name)")
    parser.add_argument('-o', '--outpath', default='./output.iso',
        help="Name of the ISO to generate. (default: %(default)s)")
    parser.add_argument('--no-par2', action="store_true",
        default=False, help="Don't generate .par2 files")
    # Reminder: %(default)s can be used in help strings.

    args = parser.parse_args()

    # Set up clean logging to stderr
    log_levels = [logging.CRITICAL, logging.ERROR, logging.WARNING,
                  logging.INFO, logging.DEBUG]
    args.verbose = min(args.verbose - args.quiet, len(log_levels) - 1)
    args.verbose = max(args.verbose, 0)
    logging.basicConfig(level=log_levels[args.verbose],
                        format='%(levelname)s: %(message)s')

    # TODO: Split all this out into its own function
    out_parent = os.path.dirname(os.path.abspath(args.outpath))
    temp_dir = tempfile.mkdtemp(prefix='balloon_cd-', dir=out_parent)
    try:
        for path in args.inpath:
            if not os.path.exists(path):
                log.warning("Input path does not exist: %s", path)
                continue

            process(path, temp_dir)

        for fname in sorted(os.listdir(temp_dir)):
            temp_path = os.path.join(temp_dir, fname)
            if temp_path.endswith('.par2'):
                log.debug("Not generating .par2.par2: %r", temp_path)
            else:
                log.info("Applying par2 to %r", temp_path)
                if not args.no_par2:
                    parchive(temp_path)

        volume_id = args.volid
        if not volume_id:
            volume_id = os.path.basename(args.inpath[0])[:32]

        generate_iso(temp_dir, args.outpath, volume_id)
    finally:
        shutil.rmtree(temp_dir)

    # TODO: Concatenate copies of the ISO to fill the remaining available space
    #       so that, if the disc degrades so badly that 200% DVDisaster
    #       redundancy isn't enough, there will still be a slight chance that
    #       it can be recovered by using a ddrescue log and some custom
    #       scripting to merge the recovered bytes from the multiple copies of
    #       the ECC-padded ISO that got burned to the disc.


if __name__ == '__main__':
    main()

# vim: set sw=4 sts=4 expandtab :
