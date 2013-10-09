// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "starfield.h"
#include "draw.h"

#include <iostream>

using namespace std;

inline float randColor()
{
	return (float) ((rand() % 100) + 155) / 255.0;
}

const unsigned int NUM_STARS = 2000;

Star	stars[NUM_STARS];

void StarField::init()
{
	for (unsigned int i = 0; i < NUM_STARS; i++)
	{
		stars[i].r = randColor();
		stars[i].g = randColor();
		stars[i].b = randColor();
		stars[i].a = randColor();
		stars[i].pos.x = (rand() % (Screen::maxX()+1000)) - 500;
		stars[i].pos.y = (rand() % (Screen::maxY()+1000)) - 500;
		stars[i].pos.z = -(rand() % 200) - 200;
	}
}

int SFS = -1;

void StarField::draw()
{
	if (SFS == -1)
	{
		SFS = glGenLists(1);
		glNewList(SFS, GL_COMPILE);
		draw::startPoints();
		for (unsigned int i = 0; i < NUM_STARS; i++)
		{
			draw::setColor(stars[i].r, stars[i].g, stars[i].b, stars[i].a);
			draw::point(stars[i].pos.x, stars[i].pos.y, stars[i].pos.z);
		}
		draw::endPoints();
		glEndList();
	}
	glCallList(SFS);
}
