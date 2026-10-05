Welcome to SSC.

This is an early release (let's say "technology demo"). The game is not fine
tuned and there is only a simple change when progressing from level to level,
so please set expectations accordingly.


REQUIREMENTS
==

Before you can compile and play SSC there are a few requirements. You must have
hardware graphics acceleration, or else the game will be painfully slow. As far
as libraries are concerned, you should have GLU, and GL (usually present).
You will need SDL and SDL_mixer (for sound) as well. pthread is included in my
link list, but you may not have or need it. You can modify the Makefile in this
case. You will also need FreeType2 for TrueType font support.


USER INFORMATION
==

If you are looking for a quick five minutes of blast-em-up action, you'll have
to get this thing compiled first. The basic steps are:

	cd src
	make

And this should produce an executable 'ssc' which can be run directly. The game
is completely self-contained.

To play, you can use the arrow keys to move and the key 'A' to fire, as such:

	a	fire
	s	toggle slow-motion
	c	cycle camera position
	r	toggle radar

	[up]	accelerate
	[down]	decelerate (quickly)
	[left]	rotate left
	[right] rotate right

	[pause]	toggle game pause
	[F1]	toggle full-screen
	[ESC]	exit game

There is some code for mouse control, but it isn't an optimal configuration:

	Button	Action
	------	------
	[left]	fire
	[middle] decelerate
	[right]	accelerate

Whenever I play with the mouse, I usually use the keyboard to accelerate and
the mouse to aim and fire.


General tips:

	- Bogeys (blue) move in flocks. Try to single them out for quick kills
	  as they are more dangerous when they are near death

	- Lunatics (white) have a negative mass, and can cling on to you,
	  causing massive damage. Try to kill them before they touch you,
	  otherwise, try accelerating/rotating/firing near walls.

	- Fatso's (beige?) don't really do anything, but are very massive. You
	  may be able to use them to your advantage if you can get bogey's to
	  collide with them. Beware of Lunatics clinging on to Fatsos!

	- Blackholes (green - go figure) suck you in and cause massive damage.
	  They also affect Bogeys to a small degree, so you can orbit a
	  Blackhole and try to lure Bogeys to their deaths.

	- Smarties (grey) learn to fly as the game progresses - actually, they
	  usually fly pretty well within the first ten seconds.. Smarties are
	  not too dangerous, but mostly annoying; they constantly bash into you
	  and can make you an easy target for bogies.


DEVELOPER INFORMATION
==

I am looking for input into the project. Goals are:

	- maintain and improve the clarity of the current code base (maybe
	  with a few more comments too)

	- rely as little as possible on external data packs

	- add more flashy graphics (how about a camera sweep introduction?)

	- come up with level ideas and goals

	- design new types of enemies

	- support multiplayer and network

	- add some more support platforms


General game idea goals are a physics based game (not necessarily entirely
composed of circles, but probably 2D) which features interesting enemies,
intense gameplay, and AI/a-life themes.

I am open to suggestions!


CREDITS
==

AUTHORS:

This game is written by Thomas D. Marsh <thomas.marsh@seznam.cz>


BORROWINGS:

The physics engine is ODE (Open Dynamics Engine) by Russell Smith.
You can download it at http://opende.sourceforge.net/

The fonts come from Divide By Zero (http://fonts.tom7.com/).

The asteroid code has been adapted by the instructive game "A Steroid"
by Paolo Cignoni. He can be reached at cignoni@iei.pi.cnr.it, and the
game at http://vcg.iei.pi.cnr.it/~cignoni/a_steroid/index.html.

TrueType support is achieved with a massively stripped down OGLFT which
can be found at http://oglft.sourceforge.net/.

The PNG opengl texture loading routines are from Ben Wyatt's glpng.[ch].
He can be reached at ben@wyatt100.freeserve.co.uk.

The simple three-layer backprop neural nets are from Daniel Franklin's
libneural; email: d.franklin@computer.org
http://ieee.uow.edu.au/~daniel/software/libneural/.

The audio code was used from Chromium BSU and is Copyright (c) 2000 Mark B.
Allan. Check it out at http://www.reptilelabour.com/software/chromium/.
