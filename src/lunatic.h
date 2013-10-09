// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_LUNATIC_H
#define SSC_LUNATIC_H

#include "object.h"
#include "explode.h"


class Lunatic : public ScreenObject
{
private:
	Lunatic(Lunatic&);
	Explosion<10> mExplosion;

public:
	Lunatic();
	~Lunatic();

	void draw();

	void rotate(double amt);
	void move(double dt);
	bool collision(ScreenObject &other);

	double alpha;
	bool dir;
};


#endif // SSC_LUNATIC_H
