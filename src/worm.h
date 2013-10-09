// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_WORM_H
#define SSC_WORM_H

#include "object.h"
#include "explode.h"


class Worm : public ScreenObject
{
private:
	Worm(Worm&);
	Explosion<10> mExplosion;

public:
	Worm();
	~Worm();

	void draw();

	void rotate(double amt);
	void move(double dt);
	bool collision(ScreenObject &other);

	double alpha;
	bool dir;

	Worm *next, *prev;
};


#endif // SSC_WORM_H
