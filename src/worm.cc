// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "worm.h"
#include "draw.h"
#include "global.h"

const unsigned int WORM_MAX_SPEED = 7;
const unsigned int WORM_RADIUS = 7;
const double WORM_MASS = -50.0;


Worm::Worm()
	: ScreenObject(WORM_TYPE,
		       WORM_RADIUS,
		       WORM_MASS,
		       WORM_MAX_SPEED,
	               (double) (rand() % 500),
		       (double) (rand() % 500),
		       0,
		       0, 0, 0)
{
	r = .9, g = .9, b = .9;
	alpha = (double) (rand() % 70) / 100;
	dir = true;

	setState(ALIVE);
}

Worm::~Worm()
{
}


void Worm::rotate(double amt)
{
	rotation += amt;

	while (rotation > D_PI)    rotation -= D_PI;
	while (rotation < 0)      rotation += D_PI;
}


void Worm::move(double dt)
{
	if (isAlive())
	{
		if (dir)
		{
			alpha -= .01 * dt;
			if (alpha <= 0)
				alpha = 0, dir = !dir;
		}
		else
		{
			alpha += .01 * dt;
			if (alpha >= .7)
				alpha = .7, dir = !dir;
		}

		
		if (onScreen() && Global::ship->isAlive())
		{
			if (speed > mMaxSpeed)
			{
				decelerate(dt);
				ScreenObject::move(dt);
				return;
			}
			Coord3<double> mForce(mVelocity);
			mForce += (mPosition-Global::ship->mPosition)/speed/2;
			rotation = atan2(mForce.x, -mForce.y);
			accelerate(dt);
		}
		else
		{
			rotate((float) ((rand() % 201) - 100) / 100.0);
			accelerate((float) (rand() % 400) / 1000.0);
		}
	}
	else if (isDying())
	{
		if (mExplosion.finished)
			setState(DEAD);
		mExplosion.move(dt);
	}
	ScreenObject::move(dt);
}

bool Worm::collision(ScreenObject& obj)
{
	static float pos[3] = {0.0, 0.0, 0.0};

	if ((obj.type() == MISSILE_TYPE) &&
	    (obj.ownerType() == PLAYER_TYPE))
	{
		setState(DYING);
		Global::audio->playSound(Audio::EXPLO_POP, pos);
		mExplosion.init(*this);
	}
	return true;
}

void Worm::draw()
{
	if (isAlive())
	{
		draw::setColor(r,g,b, alpha);
		draw::sphere(mPosition, radius);
	}
	else if (isDying())
		mExplosion.draw();
}
