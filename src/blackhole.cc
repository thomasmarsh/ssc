// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "blackhole.h"
#include "draw.h"
#include "graph.h"

BlackHole::BlackHole(double x, double y)
	: ScreenObject(BLACKHOLE_TYPE, BLACKHOLE_RADIUS, BLACKHOLE_MASS, 0,
		       x, y, 0,
		       0, 0, 0)
{
	mGravity = true;

	r = 0, g = 1, b = 0;
	setState(ALIVE);
}

BlackHole::~BlackHole()
{
}


void BlackHole::draw()
{
	draw::setColor(r,g,b,1.0);
	draw::sphere(mPosition, radius);
}
